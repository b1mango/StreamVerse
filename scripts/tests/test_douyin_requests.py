from __future__ import annotations

import sys
import unittest
from pathlib import Path
from unittest.mock import AsyncMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import douyin_bridge as d
import httpx


class DouyinRequestTests(unittest.IsolatedAsyncioTestCase):
    async def test_detail_uses_minimal_parameters_and_open_platform_headers(self):
        def handle(request):
            self.assertEqual(dict(request.url.params), {'aid': '6383', 'version_code': '290100', 'version_name': '29.1.0', 'aweme_id': '123'})
            self.assertEqual(request.headers['origin'], 'https://open.douyin.com')
            self.assertEqual(request.headers['referer'], 'https://open.douyin.com/')
            self.assertEqual(request.headers['uifid'], 'test-id')
            return httpx.Response(200, json={'status_code': 0, 'aweme_detail': {'aweme_id': '123'}})
        async with httpx.AsyncClient(transport=httpx.MockTransport(handle),
                                    headers=d.douyin_web_headers('x=1;UIFID=test-id')) as client:
            await d.fetch_one_video_fast(client, '123')

    async def test_rejects_empty_wrong_target_and_business_errors(self):
        for payload in ({}, {'aweme_detail': {'aweme_id': 'other'}}, {'status_code': 4}):
            async with httpx.AsyncClient(transport=httpx.MockTransport(
                    lambda request: httpx.Response(200, json=payload))) as client:
                with self.assertRaises(RuntimeError):
                    await d.fetch_one_video_fast(client, '123')

    async def test_profile_preserves_pagination_after_transient_403(self):
        requests = []
        def handle(request):
            requests.append(request)
            if len(requests) == 1:
                return httpx.Response(403, text='Blocked by ArgusSecurityPlugin Signature Not Found')
            self.assertEqual(dict(request.url.params), {
                'aid': '6383', 'version_code': '290100', 'version_name': '29.1.0', 'sec_user_id': 'author', 'max_cursor': '987', 'count': '20'})
            return httpx.Response(200, json={'status_code': 0, 'aweme_list': [{'aweme_id': '2'}],
                                          'max_cursor': 876, 'has_more': True})
        async with httpx.AsyncClient(transport=httpx.MockTransport(handle)) as client:
            with patch.object(d.asyncio, 'sleep', new=AsyncMock()):
                result = await d._fetch_user_posts_fast(client, 'author', 987)
        self.assertEqual(result['max_cursor'], 876)
        self.assertEqual(len(requests), 2)

    async def test_403_is_not_reported_as_timeout_or_leaks_signed_url(self):
        async with httpx.AsyncClient(transport=httpx.MockTransport(
                lambda request: httpx.Response(403))) as client:
            with self.assertRaisesRegex(RuntimeError, 'HTTP 403') as error:
                await d.fetch_one_video_fast(client, '123')
            self.assertNotIn('超时', str(error.exception))
            self.assertNotIn('https://', str(error.exception))


class DouyinDescriptionTests(unittest.TestCase):
    def test_recovers_complete_caption_instead_of_removing_warning(self):
        self.assertEqual(d.extract_description({
            'desc': '标题 正文……版本过低，升级后可展示全部信息',
            'item_title': '标题', 'caption': '正文 #话题一 #话题二',
        }), '标题 正文 #话题一 #话题二')

    def test_preserves_normal_description_and_does_not_duplicate_title(self):
        self.assertEqual(d.extract_description({'desc': '完整原文', 'caption': '其他'}), '完整原文')
        self.assertEqual(d.extract_description({
            'desc': '……版本过低，升级后可展示全部信息',
            'item_title': '标题', 'caption': '标题 正文',
        }), '标题 正文')

    def test_does_not_claim_recovery_without_full_content(self):
        desc = '正文……版本过低，升级后可展示全部信息'
        self.assertEqual(d.extract_description({'desc': desc}), desc)
