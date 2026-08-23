from __future__ import annotations

import sys
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

def main(argv: list[str]) -> int:
    if not argv:
        print("missing helper command", file=sys.stderr)
        return 2

    command, *rest = argv
    if command == "health":
        print("streamverse-helper 1.0.0")
        return 0
    if command == "douyin-analyze":
        import douyin_bridge

        return douyin_bridge.main(["analyze", *rest])
    if command == "douyin-profile":
        import douyin_bridge

        return douyin_bridge.main(["profile", *rest])
    if command == "bilibili-profile":
        import bilibili_profile_bridge

        return bilibili_profile_bridge.main(rest)
    print(f"unsupported helper command: {command}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
