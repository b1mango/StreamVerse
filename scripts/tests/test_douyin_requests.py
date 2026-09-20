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
            self.assertEqual(dict(request.url.params), {'aid': '6383', 'aweme_id': '123'})
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
                'aid': '6383', 'sec_user_id': 'author', 'max_cursor': '987', 'count': '20'})
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
