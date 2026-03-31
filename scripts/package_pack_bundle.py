#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import zipfile
from pathlib import Path


def parse_mapping(raw: str) -> tuple[Path, str]:
    if ":" not in raw:
        raise argparse.ArgumentTypeError("映射参数必须是 source:dest 形式。")
    source, dest = raw.split(":", 1)
    return Path(source), dest.strip("/")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pack-id", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--binary-path")
    parser.add_argument("--binary-name")
    parser.add_argument("--output", required=True)
    parser.add_argument("--resource-only", action="store_true")
    parser.add_argument("--include-file", action="append", default=[], type=parse_mapping)
    parser.add_argument("--include-dir", action="append", default=[], type=parse_mapping)
    return parser.parse_args()


def add_file(bundle: zipfile.ZipFile, source: Path, destination: str) -> None:
    if source.is_file():
        bundle.write(source, destination)


def add_directory(bundle: zipfile.ZipFile, source: Path, destination: str) -> None:
    if not source.is_dir():
        return
    for path in source.rglob("*"):
        if path.is_dir() or "__pycache__" in path.parts:
            continue
        relative = path.relative_to(source).as_posix()
        bundle.write(path, f"{destination}/{relative}")


def main() -> None:
    args = parse_args()
    binary_path = Path(args.binary_path) if args.binary_path else None
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    binary_name = args.binary_name or (binary_path.name if binary_path else args.pack_id)

    manifest = {
        "id": args.pack_id,
        "version": args.version,
        "binaryName": binary_name,
        "resourceOnly": args.resource_only,
    }

    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
        bundle.writestr("manifest.json", json.dumps(manifest, ensure_ascii=False, indent=2))
        if binary_path:
            add_file(bundle, binary_path, f"bin/{binary_path.name}")
        for source, destination in args.include_file:
            add_file(bundle, source, destination)
        for source, destination in args.include_dir:
            add_directory(bundle, source, destination)


if __name__ == "__main__":
    main()
