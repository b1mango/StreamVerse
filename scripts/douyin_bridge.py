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
import os
import re
import sys
from datetime import datetime
from pathlib import Path
from typing import Any

REPO_ROOT = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parents[1]))
VENDOR_ROOT = REPO_ROOT / "vendor" / "douyin_api"
DESKTOP_UA = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/137.0.0.0 Safari/537.36"
)
DOUYIN_REFERER = "https://www.douyin.com/"
IMAGE_AWEME_TYPES = {2, 68}
SINGLE_ANALYZE_MAX_RETRIES = 4
PROFILE_REQUEST_UA = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36"
)
PROGRESS_FILE = os.environ.get("STREAMVERSE_PROGRESS_FILE")

if str(VENDOR_ROOT) not in sys.path:
    sys.path.insert(0, str(VENDOR_ROOT))

import httpx  # noqa: E402
from urllib.parse import urlencode  # noqa: E402

from crawlers.douyin.web.endpoints import DouyinAPIEndpoints  # noqa: E402
from crawlers.douyin.web.models import PostDetail, UserPost  # noqa: E402
from crawlers.douyin.web.utils import (  # noqa: E402
    AwemeIdFetcher,
    BogusManager,
    TokenManager,
    config as utils_config,
)
from crawlers.douyin.web.web_crawler import (  # noqa: E402
    DouyinWebCrawler,
    config as crawler_config,
)


def ensure_utf8_stdio() -> None:
    for stream_name in ("stdout", "stderr"):
        stream = getattr(sys, stream_name, None)
        if stream is None:
            continue
        reconfigure = getattr(stream, "reconfigure", None)
        if callable(reconfigure):
            reconfigure(encoding="utf-8", errors="replace")


ensure_utf8_stdio()


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


def write_progress(current: int, total: int, message: str) -> None:
    if not PROGRESS_FILE:
        return

    path = Path(PROGRESS_FILE)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f"{path.name}.{os.getpid()}.tmp")
    temporary.write_text(
        json.dumps(
            {
                "current": max(0, int(current)),
                "total": max(int(total), int(current), 1),
                "message": message,
            },
            ensure_ascii=False,
        ),
        "utf-8",
    )
    os.replace(temporary, path)


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
        lowered = item.lower()
        if "audio" in lowered and "/video/" not in lowered:
            continue
        if item.endswith(".mp4") or "/video/" in item or "aweme/v1/play/" in item:
            return item

    for item in url_list:
        lowered = item.lower()
        if "audio" not in lowered:
            return item

    return url_list[0]


def first_url_from_candidate(candidate: Any) -> str | None:
    if isinstance(candidate, str) and candidate.strip():
        return candidate

    if isinstance(candidate, dict):
        url_list = candidate.get("url_list") or candidate.get("urlList") or []
        if isinstance(url_list, list):
            for item in url_list:
                if isinstance(item, str) and item.strip():
                    return item

    if isinstance(candidate, list):
        for item in candidate:
            if isinstance(item, str) and item.strip():
                return item

    return None


def is_known_watermarked_image_url(url: str) -> bool:
    normalized = url.lower()
    return "tplv-dy-water" in normalized or "owner_watermark" in normalized


def extract_cover_urls(detail: dict[str, Any]) -> list[str]:
    if int(detail.get("aweme_type") or 0) in IMAGE_AWEME_TYPES:
        image_urls = extract_image_urls(detail)
        if image_urls:
            return [image_urls[0]]

    urls: list[str] = []
    top_video = detail.get("video") or {}
    # `cover` is the feed artwork. The other fields frequently expose the original
    # portrait canvas or a generated video frame, so only use them as fallbacks.
    for key in ("cover", "cover_original_scale", "dynamic_cover", "origin_cover"):
        cover_url = first_url_from_candidate(top_video.get(key))
        if cover_url and cover_url not in urls:
            urls.append(cover_url)

    for cover_url in extract_image_urls(detail):
        if cover_url not in urls:
            urls.append(cover_url)

    return urls


def extract_cover_url(detail: dict[str, Any]) -> str | None:
    return next(iter(extract_cover_urls(detail)), None)


def extract_image_urls(detail: dict[str, Any]) -> list[str]:
    urls: list[str] = []
    seen: set[str] = set()
    for image in detail.get("images") or []:
        if not isinstance(image, dict):
            continue

        candidates = (
            image.get("watermark_free_download_url_list"),
            image.get("origin_image"),
            image.get("display_image"),
            image.get("url_list"),
        )
        selected = next(
            (
                url
                for candidate in candidates
                if (url := first_url_from_candidate(candidate))
                and not is_known_watermarked_image_url(url)
            ),
            None,
        )
        if selected and selected not in seen:
            seen.add(selected)
            urls.append(selected)
    return urls


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
    best_formats: dict[tuple[str, str, str], dict[str, Any]] = {}
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

            format_item = {
                "id": f"{source_key}:{gear_name or pick_display_height(width, height, '')}:{index}",
                "label": build_format_label(width, height, gear_name),
                "resolution": build_resolution(width, height),
                "bitrateKbps": bitrate_kbps,
                "codec": codec,
                "container": "MP4",
                "noWatermark": True,
                "requiresLogin": using_login,
                "requiresProcessing": False,
                "recommended": False,
                "directUrl": direct_url,
                "referer": DOUYIN_REFERER,
                "userAgent": DESKTOP_UA,
                "fileSizeBytes": int(
                    play_addr.get("data_size")
                    or bit_rate.get("data_size")
                    or video.get("data_size")
                    or 0
                ) or None,
            }
            unique_key = (format_item["label"].upper(), codec, "MP4")
            existing = best_formats.get(unique_key)
            if existing is None or bitrate_kbps > int(existing["bitrateKbps"]):
                best_formats[unique_key] = format_item

    formats = list(best_formats.values())

    formats.sort(
        key=lambda item: (
            int(item["resolution"].split("x")[1]) if "x" in item["resolution"] else 0,
            int(item["bitrateKbps"]),
        ),
        reverse=True,
    )

    if formats:
        preferred = max(
            formats,
            key=lambda item: (
                item["codec"] == "H.264",
                int(item["resolution"].split("x")[1]) if "x" in item["resolution"] else 0,
                int(item["bitrateKbps"]),
            ),
        )
        preferred["recommended"] = True

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
    image_urls = extract_image_urls(detail)
    if not formats and not image_urls:
        return None

    author = (detail.get("author") or {}).get("nickname") or "未知作者"
    title = (detail.get("desc") or detail.get("item_title") or "").strip() or aweme_id

    return {
        "awemeId": aweme_id,
        "platform": "douyin",
        "sourceUrl": source_url,
        "title": title,
        "author": author,
        "durationSeconds": compute_duration_seconds(detail),
        "publishDate": format_publish_date(detail.get("create_time")),
        "caption": build_caption(detail, using_login, bool(formats)),
        "categoryLabel": "图文笔记" if int(detail.get("aweme_type") or 0) in IMAGE_AWEME_TYPES else "普通视频",
        "groupTitle": None,
        "coverUrl": extract_cover_url(detail),
        "coverUrls": extract_cover_urls(detail),
        "coverGradient": "linear-gradient(135deg, rgba(13, 190, 165, 0.95), rgba(97, 87, 255, 0.8))",
        "imageUrls": image_urls,
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

    if aweme_type in IMAGE_AWEME_TYPES:
        detail = "，并保留可用动态内容" if has_video_formats else ""
        return f"{prefix} 当前作品是图文笔记，已提取全部可下载图片{detail}。"

    return f"{prefix} 可以直接选择清晰度开始下载。"


PROFILE_URL_HINTS = ("/user/", "/share/user")
SHORT_LINK_HOSTS = ("v.douyin.com", "iesdouyin.com")
PROFILE_LINK_MESSAGE = "检测到这是抖音主页链接，请切换到「主页批量」模块解析。"
SINGLE_ANALYZE_TIMEOUT = 10
SINGLE_ANALYZE_RETRY_DELAY = 0.5

# 与 AwemeIdFetcher 相同的匹配顺序，用于本地提取完整链接的作品 ID
_AWEME_ID_URL_PATTERNS = (
    re.compile(r"video/([^/?]*)"),
    re.compile(r"[?&]vid=(\d+)"),
    re.compile(r"note/([^/?]*)"),
    re.compile(r"modal_id=([0-9]+)"),
)


def extract_aweme_id_from_url(url: str) -> str | None:
    for pattern in _AWEME_ID_URL_PATTERNS:
        match = pattern.search(url)
        if match:
            return match.group(1)
    return None


async def ensure_not_profile_link(url: str) -> str:
    """拦截主页链接；短链顺带解析重定向并返回最终 URL。"""
    lowered = url.lower()
    if any(hint in lowered for hint in PROFILE_URL_HINTS):
        raise RuntimeError(PROFILE_LINK_MESSAGE)

    if not any(host in lowered for host in SHORT_LINK_HOSTS):
        return url

    try:
        async with httpx.AsyncClient(
            proxy=None, timeout=10, follow_redirects=True
        ) as client:
            response = await client.get(url)
        final_url = str(response.url)
    except httpx.HTTPError:
        return url

    if any(hint in final_url.lower() for hint in PROFILE_URL_HINTS):
        raise RuntimeError(PROFILE_LINK_MESSAGE)

    return final_url


async def resolve_aweme_id(url: str) -> str:
    final_url = await ensure_not_profile_link(url)
    # 完整链接本地提取作品 ID，省掉一次重定向请求；提取不到再回退到网络请求
    aweme_id = extract_aweme_id_from_url(final_url)
    if aweme_id:
        return aweme_id
    return await AwemeIdFetcher.get_aweme_id(final_url)


async def fetch_one_video_fast(
    client: httpx.AsyncClient, user_agent: str, aweme_id: str, ms_token: str
) -> dict[str, Any]:
    """单作品详情请求，复用共享 client。该接口对空 msToken 的 403 拦截是间歇性的，
    重试时由调用方轮换真实 msToken（与主页批量路径一致）。"""
    params = PostDetail(aweme_id=aweme_id)
    params_dict = params.dict()
    params_dict["msToken"] = ms_token
    a_bogus = BogusManager.ab_model_2_endpoint(params_dict, user_agent)
    endpoint = f"{DouyinAPIEndpoints.POST_DETAIL}?{urlencode(params_dict)}&a_bogus={a_bogus}"
    response = await client.get(endpoint, follow_redirects=True)
    response.raise_for_status()
    return response.json()


async def analyze(url: str, cookie_file: Path | None) -> dict[str, Any]:
    write_progress(0, 4, "正在准备抖音解析环境…")
    patch_cookie_config(build_cookie_header(cookie_file))
    aweme_id = await resolve_aweme_id(url)
    write_progress(1, 4, "正在读取抖音作品信息…")

    crawler = DouyinWebCrawler()
    kwargs = await crawler.get_douyin_headers()
    headers = kwargs["headers"]
    user_agent = headers["User-Agent"]

    response: dict[str, Any] | None = None
    last_error: Exception | None = None
    async with httpx.AsyncClient(
        headers=headers,
        proxies=kwargs.get("proxies"),
        timeout=httpx.Timeout(SINGLE_ANALYZE_TIMEOUT),
        transport=httpx.AsyncHTTPTransport(retries=3),
    ) as client:
        for attempt in range(1, SINGLE_ANALYZE_MAX_RETRIES + 1):
            # 首次请求沿用空 msToken 快速路径；失败后轮换真实 msToken 规避间歇性 403
            ms_token = "" if attempt == 1 else TokenManager.gen_real_msToken()
            try:
                response = await fetch_one_video_fast(
                    client, user_agent, aweme_id, ms_token
                )
                break
            except Exception as error:
                last_error = error
                if attempt >= SINGLE_ANALYZE_MAX_RETRIES:
                    break
                write_progress(
                    1,
                    4,
                    f"抖音作品信息读取超时，正在进行第 {attempt + 1} 次重试…",
                )
                await asyncio.sleep(SINGLE_ANALYZE_RETRY_DELAY * attempt)

    if response is None:
        raise RuntimeError(f"读取抖音作品信息失败：{last_error}")

    detail = response.get("aweme_detail") or {}
    write_progress(2, 4, "正在整理抖音可用清晰度…")

    asset = build_asset_from_detail(
        detail,
        source_url=build_source_url(detail, url),
        using_login=bool(cookie_file),
    )
    write_progress(3, 4, "正在生成抖音预览结果…")
    if not asset:
        raise RuntimeError("当前作品未返回可下载的视频或图片资源。")

    write_progress(4, 4, "抖音作品解析完成。")
    return asset


def extract_cookie_value(cookie_header: str, name: str) -> str:
    for pair in cookie_header.split("; "):
        key, separator, value = pair.partition("=")
        if separator and key.strip() == name:
            return value.strip()
    return ""


PROFILE_POST_MAX_ATTEMPTS = 4


async def _fetch_user_posts_fast(
    client: httpx.AsyncClient,
    user_agent: str,
    sec_user_id: str,
    max_cursor: int,
    count: int = 20,
    uifid: str = "",
    verify_fp: str = "",
) -> dict[str, Any]:
    """Fetch user post videos reusing a shared httpx.AsyncClient.

    抖音风控对该接口的 403 拦截是间歇性的（相同参数时好时坏），
    做应用级重试并轮换 msToken，持续失败才抛出可操作的错误信息。
    """
    for attempt in range(1, PROFILE_POST_MAX_ATTEMPTS + 1):
        params = UserPost(sec_user_id=sec_user_id, max_cursor=max_cursor, count=count)
        params_dict = params.dict()
        params_dict["msToken"] = "" if attempt == 1 else TokenManager.gen_real_msToken()
        extra_headers: dict[str, str] = {}
        if uifid:
            params_dict["uifid"] = uifid
            params_dict["verifyFp"] = verify_fp
            params_dict["fp"] = verify_fp
            extra_headers["uifid"] = uifid
        a_bogus = BogusManager.ab_model_2_endpoint(params_dict, user_agent)
        endpoint = f"{DouyinAPIEndpoints.USER_POST}?{urlencode(params_dict)}&a_bogus={a_bogus}"
        response = await client.get(endpoint, follow_redirects=True, headers=extra_headers)
        if response.status_code != 403:
            response.raise_for_status()
            return response.json()
        if attempt < PROFILE_POST_MAX_ATTEMPTS:
            await asyncio.sleep(attempt)
            continue
        if "Uifid Not Found" in response.text:
            raise RuntimeError(
                "抖音网页风控参数缺失，请重新从浏览器导入抖音 Cookie 后再试。"
            )
        raise RuntimeError(
            "抖音网页接口暂时被风控拦截，已自动重试仍失败。请等 1-2 分钟后重试；"
            "若持续失败，请重新登录抖音并重新导入 Cookie。"
        )

    raise RuntimeError("抖音主页作品读取失败。")


async def analyze_profile(
    url: str, cookie_file: Path | None, limit: int
) -> dict[str, Any]:
    cookie_header = build_cookie_header(cookie_file)
    patch_cookie_config(cookie_header)
    using_login = bool(cookie_file)
    normalized_limit = max(1, min(limit, 2000))
    uifid = extract_cookie_value(cookie_header, "UIFID")
    verify_fp = extract_cookie_value(cookie_header, "s_v_web_id")

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
    progress_total = max(1, min(total_available or normalized_limit, normalized_limit))
    write_progress(0, progress_total, "正在读取抖音主页作品（含视频与图册）…")

    items: list[dict[str, Any]] = []
    seen_aweme_ids: set[str] = set()
    skipped_count = 0
    max_cursor = 0
    has_more = True

    kwargs = await crawler.get_douyin_headers()
    # 请求头 UA 必须与签名指纹参数（browser_version=130.0.0.0）一致，否则 a_bogus 校验失败
    profile_headers = dict(kwargs["headers"])
    profile_headers["User-Agent"] = PROFILE_REQUEST_UA
    user_agent = PROFILE_REQUEST_UA

    async with httpx.AsyncClient(
        headers=profile_headers,
        proxies=kwargs.get("proxies"),
        timeout=httpx.Timeout(15),
        limits=httpx.Limits(max_connections=50),
        transport=httpx.AsyncHTTPTransport(retries=3),
    ) as client:
        while has_more and len(items) < normalized_limit:
            response = await _fetch_user_posts_fast(
                client, user_agent, sec_user_id, max_cursor, count=20,
                uifid=uifid, verify_fp=verify_fp,
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
                    write_progress(len(items), progress_total, f"已解析 {len(items)} 个抖音作品（含视频与图册）。")
                else:
                    skipped_count += 1

            next_cursor = int(response.get("max_cursor") or 0)
            has_more = bool(response.get("has_more")) and next_cursor != max_cursor
            max_cursor = next_cursor

    if not items:
        raise RuntimeError("当前主页暂时没有可批量下载的作品，或需要更新登录状态后重试。")

    write_progress(len(items), progress_total, "抖音主页解析完成。")

    return {
        "profileTitle": str(profile_title),
        "sourceUrl": url,
        "secUserId": sec_user_id,
        "totalAvailable": max(total_available, len(items) + skipped_count),
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
