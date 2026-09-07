from __future__ import annotations

import subprocess
import sys
from pathlib import Path


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    separator = ";" if sys.platform == "win32" else ":"
    # macOS 上 onefile 每次启动都要重新自解压并被安全子系统扫描（实测 17-56s），
    # 改用 onedir：首次扫描后稳定路径可缓存，启动约 0.03s
    mode = "--onefile" if sys.platform == "win32" else "--onedir"
    command = [
        sys.executable,
        "-m",
        "PyInstaller",
        "--noconfirm",
        "--clean",
        mode,
        "--name",
        "streamverse-helper",
        "--distpath",
        str(root / "scripts" / "dist"),
        "--workpath",
        str(root / "scripts" / "build"),
        "--specpath",
        str(root / "scripts" / "build"),
        "--paths",
        str(root / "scripts"),
        "--paths",
        str(root / "vendor" / "douyin_api"),
        "--add-data",
        f"{root / 'vendor' / 'douyin_api'}{separator}vendor/douyin_api",
        str(root / "scripts" / "streamverse_helper.py"),
    ]
    return subprocess.run(command, cwd=root, check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
