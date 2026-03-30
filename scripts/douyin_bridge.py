#!/usr/bin/env python3
"""Bridge Douyin share links into direct downloadable formats.

This script relies on vendored upstream crawlers from Evil0ctal's
Douyin_TikTok_Download_API project under /vendor/douyin_api.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import math
import re
import sys
from datetime import datetime
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
VENDOR_ROOT = REPO_ROOT / "vendor" / "douyin_api"
DESKTOP_UA = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/137.0.0.0 Safari/537.36"
)
DOUYIN_REFERER = "https://www.douyin.com/"
IMAGE_AWEME_TYPES = {2, 68}

if str(VENDOR_ROOT) not in sys.path:
    sys.path.insert(0, str(VENDOR_ROOT))

from crawlers.douyin.web.utils import AwemeIdFetcher, config as utils_config  # noqa: E402
from crawlers.douyin.web.web_crawler import (  # noqa: E402
    DouyinWebCrawler,
    config as crawler_config,
)


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    analyze_parser = subparsers.add_parser("analyze")
    analyze_parser.add_argument("--url", required=True)
    analyze_parser.add_argument("--cookie-file")

    profile_parser = subparsers.add_parser("profile")
    profile_parser.add_argument("--url", required=True)
    profile_parser.add_argument("--cookie-file")
    profile_parser.add_argument("--limit", type=int, default=24)

    return parser.parse_args(argv)


def build_cookie_header(cookie_file: Path | None) -> str:
    if not cookie_file or not cookie_file.exists():
        return ""

    pairs: list[str] = []
    for raw_line in cookie_file.read_text("utf-8").splitlines():
        if not raw_line or raw_line.startswith("# "):
            continue

        line = (
            raw_line[len("#HttpOnly_") :]
            if raw_line.startswith("#HttpOnly_")
            else raw_line
        )
        columns = line.split("\t")
        if len(columns) < 7:
            continue

        domain, _include_subdomains, _path, _secure, _expires, name = columns[:6]
        value = "\t".join(columns[6:])
        if "douyin.com" not in domain and "iesdouyin.com" not in domain:
            continue

        pairs.append(f"{name}={value}")

    return "; ".join(pairs)


def patch_cookie_config(cookie_header: str) -> None:
    for config in (crawler_config, utils_config):
        headers = config["TokenManager"]["douyin"]["headers"]
        headers["Cookie"] = cookie_header
        headers["Referer"] = DOUYIN_REFERER


def format_publish_date(raw_timestamp: Any) -> str:
    try:
        timestamp = int(raw_timestamp)
    except (TypeError, ValueError):
        return "未知"

    return datetime.fromtimestamp(timestamp).strftime("%Y-%m-%d")


def choose_direct_url(url_list: list[str]) -> str | None:
    if not url_list:
        return None

    for item in url_list:
        if item.endswith(".mp4") or "/video/" in item or "aweme/v1/play/" in item:
            return item

    return url_list[0]


def pick_display_height(width: int, height: int, gear_name: str) -> int:
    if gear_name:
        match = re.search(r"(\d{3,4})", gear_name)
        if match:
            return int(match.group(1))

    if width and height:
        return min(width, height)

    return height or width or 0


def build_format_label(width: int, height: int, gear_name: str) -> str:
    display_height = pick_display_height(width, height, gear_name)
    if display_height:
        return f"{display_height}P"
    if gear_name:
        return gear_name
    return "标准"


def build_resolution(width: int, height: int) -> str:
    if width and height:
        return f"{width}x{height}"
    return "Auto"


def collect_video_sources(detail: dict[str, Any]) -> list[tuple[str, dict[str, Any]]]:
    sources: list[tuple[str, dict[str, Any]]] = []
    top_video = detail.get("video") or {}
    if top_video:
        sources.append(("video", top_video))

    for index, image in enumerate(detail.get("images") or []):
        image_video = (image or {}).get("video") or {}
        if image_video:
            sources.append((f"image:{index}", image_video))

    return sources


def collect_formats(detail: dict[str, Any], using_login: bool) -> list[dict[str, Any]]:
    formats: list[dict[str, Any]] = []
    seen_urls: set[str] = set()

    for source_key, video in collect_video_sources(detail):
        bit_rates = video.get("bit_rate") or []
        for index, bit_rate in enumerate(bit_rates):
            play_addr = bit_rate.get("play_addr") or {}
            url_list = play_addr.get("url_list") or []
            direct_url = choose_direct_url(url_list)
            if not direct_url or direct_url in seen_urls:
                continue

            seen_urls.add(direct_url)
            width = int(play_addr.get("width") or video.get("width") or 0)
            height = int(play_addr.get("height") or video.get("height") or 0)
            gear_name = str(bit_rate.get("gear_name") or "")
            bitrate_kbps = int(round((bit_rate.get("bit_rate") or 0) / 1000))
            codec = "H.265" if bit_rate.get("is_h265") or video.get("is_h265") else "H.264"

            formats.append(
                {
                    "id": f"{source_key}:{gear_name or pick_display_height(width, height, '')}:{index}",
                    "label": build_format_label(width, height, gear_name),
                    "resolution": build_resolution(width, height),
                    "bitrateKbps": bitrate_kbps,
                    "codec": codec,
                    "container": "MP4",
                    "noWatermark": True,
                    "requiresLogin": using_login,
                    "recommended": False,
                    "directUrl": direct_url,
                    "referer": DOUYIN_REFERER,
                    "userAgent": DESKTOP_UA,
                }
            )

    formats.sort(
        key=lambda item: (
            int(item["resolution"].split("x")[1]) if "x" in item["resolution"] else 0,
            int(item["bitrateKbps"]),
        ),
        reverse=True,
    )

    if formats:
        formats[0]["recommended"] = True

    return formats


def build_source_url(detail: dict[str, Any], fallback_url: str) -> str:
    aweme_id = str(detail.get("aweme_id") or "").strip()
    if not aweme_id:
        return fallback_url

    if detail.get("share_url"):
        return str(detail["share_url"])

    aweme_type = int(detail.get("aweme_type") or 0)
    if aweme_type in IMAGE_AWEME_TYPES:
        return f"https://www.douyin.com/note/{aweme_id}"

    return f"https://www.douyin.com/video/{aweme_id}"


def build_asset_from_detail(
    detail: dict[str, Any], source_url: str, using_login: bool
) -> dict[str, Any] | None:
    aweme_id = str(detail.get("aweme_id") or "").strip()
    if not aweme_id:
        return None

    formats = collect_formats(detail, using_login=using_login)
    if not formats:
        return None

    author = (detail.get("author") or {}).get("nickname") or "未知作者"
    title = (detail.get("desc") or detail.get("item_title") or "").strip() or aweme_id

    return {
        "awemeId": aweme_id,
        "sourceUrl": source_url,
        "title": title,
        "author": author,
        "durationSeconds": compute_duration_seconds(detail),
        "publishDate": format_publish_date(detail.get("create_time")),
        "caption": build_caption(detail, using_login, bool(formats)),
        "coverGradient": "linear-gradient(135deg, rgba(13, 190, 165, 0.95), rgba(97, 87, 255, 0.8))",
        "formats": formats,
    }


def compute_duration_seconds(detail: dict[str, Any]) -> int:
    max_duration_ms = int(detail.get("duration") or 0)
    for _source_key, video in collect_video_sources(detail):
        max_duration_ms = max(max_duration_ms, int(video.get("duration") or 0))

    if max_duration_ms <= 0:
        return 0

    return max(1, math.ceil(max_duration_ms / 1000))


def build_caption(detail: dict[str, Any], using_login: bool, has_video_formats: bool) -> str:
    prefix = "已通过浏览器 Cookie 完成解析。" if using_login else "已通过网页接口完成解析。"
    aweme_type = int(detail.get("aweme_type") or 0)

    if aweme_type in IMAGE_AWEME_TYPES and has_video_formats:
        return f"{prefix} 该复制链接实际指向笔记作品，已提取其中可下载的动态内容。"

    if aweme_type in IMAGE_AWEME_TYPES:
        return f"{prefix} 当前链接是图文笔记，暂不支持纯图片下载。"

    return f"{prefix} 可以直接选择清晰度开始下载。"


async def analyze(url: str, cookie_file: Path | None) -> dict[str, Any]:
    patch_cookie_config(build_cookie_header(cookie_file))

    aweme_id = await AwemeIdFetcher.get_aweme_id(url)
    crawler = DouyinWebCrawler()
    response = await crawler.fetch_one_video(aweme_id)
    detail = response.get("aweme_detail") or {}

    asset = build_asset_from_detail(
        detail,
        source_url=build_source_url(detail, url),
        using_login=bool(cookie_file),
    )
    if not asset:
        raise RuntimeError("当前链接是图文笔记或受限内容，暂时没有可下载的视频格式。")

    return asset


async def analyze_profile(
    url: str, cookie_file: Path | None, limit: int
) -> dict[str, Any]:
    patch_cookie_config(build_cookie_header(cookie_file))
    using_login = bool(cookie_file)
    normalized_limit = max(1, min(limit, 100))

    crawler = DouyinWebCrawler()
    sec_user_id = await crawler.get_sec_user_id(url)
    profile_response = await crawler.handler_user_profile(sec_user_id)
    user = (
        profile_response.get("user")
        or profile_response.get("user_info")
        or profile_response.get("user_detail")
        or {}
    )

    profile_title = (
        user.get("nickname")
        or user.get("unique_id")
        or user.get("sec_uid")
        or sec_user_id
    )
    total_available = int(user.get("aweme_count") or 0)

    items: list[dict[str, Any]] = []
    seen_aweme_ids: set[str] = set()
    skipped_count = 0
    max_cursor = 0
    has_more = True

    while has_more and len(items) < normalized_limit:
        response = await crawler.fetch_user_post_videos(
            sec_user_id=sec_user_id,
            max_cursor=max_cursor,
            count=min(18, normalized_limit - len(items)),
        )

        aweme_list = response.get("aweme_list") or []
        if not aweme_list:
            break

        for detail in aweme_list:
            aweme_id = str(detail.get("aweme_id") or "").strip()
            if not aweme_id or aweme_id in seen_aweme_ids:
                continue

            seen_aweme_ids.add(aweme_id)
            asset = build_asset_from_detail(
                detail,
                source_url=build_source_url(detail, url),
                using_login=using_login,
            )
            if asset:
                items.append(asset)
                if len(items) >= normalized_limit:
                    break
            else:
                skipped_count += 1

        next_cursor = int(response.get("max_cursor") or 0)
        has_more = bool(response.get("has_more")) and next_cursor != max_cursor
        max_cursor = next_cursor

    if not items:
        raise RuntimeError("当前主页暂时没有可批量下载的视频作品，或需要更新登录状态后重试。")

    return {
        "profileTitle": str(profile_title),
        "sourceUrl": url,
        "secUserId": sec_user_id,
        "totalAvailable": total_available or len(items),
        "fetchedCount": len(items),
        "skippedCount": skipped_count,
        "items": items,
    }


async def async_main(args: argparse.Namespace) -> int:
    cookie_file = Path(args.cookie_file) if args.cookie_file else None

    if args.command == "analyze":
        payload = await analyze(url=args.url, cookie_file=cookie_file)
    elif args.command == "profile":
        payload = await analyze_profile(
            url=args.url,
            cookie_file=cookie_file,
            limit=args.limit,
        )
    else:
        raise RuntimeError(f"Unsupported command: {args.command}")

    print(json.dumps(payload, ensure_ascii=False))
    return 0


def main(argv: list[str]) -> int:
    try:
        args = parse_args(argv)
        return asyncio.run(async_main(args))
    except Exception as error:  # pragma: no cover - CLI surface
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
