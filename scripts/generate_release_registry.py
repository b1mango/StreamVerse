#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


def parse_resource_only_flag(raw: str) -> bool:
    return raw.strip().lower() in {"true", "1", "yes", "resourceonly"}


def parse_pack(raw: str) -> tuple[str, str, str, Path, bool]:
    parts = raw.split(":", 4)
    if len(parts) not in (4, 5):
        raise argparse.ArgumentTypeError("pack 参数必须是 packId:binaryName:target:path[:resourceOnly] 形式。")
    pack_id, binary_name, target, path = parts[:4]
    resource_only = len(parts) == 5 and parse_resource_only_flag(parts[4])
    return pack_id, binary_name, target, Path(path), resource_only


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    digest.update(path.read_bytes())
    return digest.hexdigest()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--pack", action="append", default=[], type=parse_pack)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    packs = []
    for pack_id, binary_name, target, path, resource_only in args.pack:
        packs.append(
            {
                "id": pack_id,
                "binaryName": binary_name,
                "version": args.version,
                "resourceOnly": resource_only,
                "target": target,
                "sizeBytes": path.stat().st_size,
                "sha256": sha256(path),
                "source": {
                    "kind": "url",
                    "url": f"https://github.com/{args.repository}/releases/download/{args.tag}/{path.name}",
                },
            }
        )

    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps({"packs": packs}, ensure_ascii=False, indent=2) + "\n", "utf-8")


if __name__ == "__main__":
    main()
