from __future__ import annotations

import subprocess
import sys
from pathlib import Path


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    separator = ";" if sys.platform == "win32" else ":"
    command = [
        sys.executable,
        "-m",
        "PyInstaller",
        "--noconfirm",
        "--clean",
        "--onefile",
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
