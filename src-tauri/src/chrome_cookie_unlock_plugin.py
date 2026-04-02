# yt-dlp plugin: ChromeCookieUnlock
# Adapted from https://github.com/seproDev/yt-dlp-ChromeCookieUnlock
# Original by Charles Machalow (MIT License)
import sys, os
if sys.platform == "win32":
    from ctypes import windll, byref, create_unicode_buffer, pointer, WINFUNCTYPE
    from ctypes.wintypes import DWORD, WCHAR, UINT
    ERROR_SUCCESS, ERROR_MORE_DATA, RmForceShutdown = 0, 234, 1
    @WINFUNCTYPE(None, UINT)
    def _rm_cb(pct):
        pass
    _rstrtmgr = windll.LoadLibrary("Rstrtmgr")
    def _unlock_file(path):
        sh = DWORD(0)
        result = DWORD(_rstrtmgr.RmStartSession(byref(sh), DWORD(0), (WCHAR * 256)())).value
        if result != ERROR_SUCCESS:
            return
        try:
            result = DWORD(_rstrtmgr.RmRegisterResources(sh, 1, byref(pointer(create_unicode_buffer(path))), 0, None, 0, None)).value
            if result != ERROR_SUCCESS:
                return
            needed = DWORD(0)
            result = DWORD(_rstrtmgr.RmGetList(sh, byref(needed), byref(DWORD(0)), None, byref(DWORD(0)))).value
            if result not in (ERROR_SUCCESS, ERROR_MORE_DATA):
                return
            if needed.value:
                _rstrtmgr.RmShutdown(sh, RmForceShutdown, _rm_cb)
        finally:
            _rstrtmgr.RmEndSession(sh)
    import yt_dlp.cookies
    _orig = yt_dlp.cookies._open_database_copy
    def _patched(db_path, tmpdir):
        try:
            return _orig(db_path, tmpdir)
        except PermissionError:
            print("[StreamVerse] Unlocking Chrome cookie database...", file=sys.stderr)
            _unlock_file(db_path)
            return _orig(db_path, tmpdir)
    yt_dlp.cookies._open_database_copy = _patched

from yt_dlp.postprocessor.common import PostProcessor
class ChromeCookieUnlockPP(PostProcessor):
    pass
