from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import bilibili_profile_bridge as b
import httpx


class EnsureApiOkTests(unittest.TestCase):
    def test_minus_101_maps_to_login_expired_message(self):
        # SESSDATA 过期时 nav 等接口返回 code -101「账号未登录」，
        # 必须归一化为登录态文案，app 侧才会触发静默重取 Cookie + 重试
        response = httpx.Response(200, json={"code": -101, "message": "账号未登录"})
        with self.assertRaisesRegex(RuntimeError, "登录态已失效"):
            b.ensure_api_ok(response, {"code": -101, "message": "账号未登录"}, "fallback")

    def test_other_business_codes_keep_original_message(self):
        response = httpx.Response(200, json={"code": -404, "message": "啥都木有"})
        with self.assertRaisesRegex(RuntimeError, "啥都木有"):
            b.ensure_api_ok(response, {"code": -404, "message": "啥都木有"}, "fallback")

    def test_success_returns_data(self):
        response = httpx.Response(200, json={"code": 0, "data": {"isLogin": True}})
        self.assertEqual(
            b.ensure_api_ok(response, {"code": 0, "data": {"isLogin": True}}, "fallback"),
            {"isLogin": True},
        )

    def test_missing_login_cookie_message_triggers_auth_refresh(self):
        # Cookie 文件存在但无登录键时，文案必须命中 app 侧登录态分类（含「登录态已失效」），
        # 才能触发静默重取浏览器 Cookie + 重试
        import contextlib
        import io
        import tempfile

        with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as file:
            file.write("# Netscape HTTP Cookie File\n.example.com\tTRUE\t/\tFALSE\t0\tfoo\tbar\n")
            cookie_path = file.name
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            code = b.main(["--url", "https://space.bilibili.com/123", "--cookie-file", cookie_path])
        self.assertEqual(code, 1)
        self.assertIn("登录态已失效", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
