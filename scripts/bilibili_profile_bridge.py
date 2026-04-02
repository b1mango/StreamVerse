#!/usr/bin/env python3
from __future__ import annotations

import asyncio
import argparse
import json
import os
import re
import sys
import time
from hashlib import md5
from pathlib import Path
from urllib.parse import urlencode, urlparse

import httpx


def ensure_utf8_stdio() -> None:
    for stream_name in ("stdout", "stderr"):
        stream = getattr(sys, stream_name, None)
        if stream is None:
            continue
        reconfigure = getattr(stream, "reconfigure", None)
        if callable(reconfigure):
            reconfigure(encoding="utf-8", errors="replace")


ensure_utf8_stdio()

DEFAULT_GRADIENT = "linear-gradient(135deg, rgba(13, 190, 165, 0.95), rgba(97, 87, 255, 0.8))"
USER_AGENT = (
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
    "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/137.0.0.0 Safari/537.36"
)
FETCH_CONCURRENCY = 20
PROGRESS_FILE = os.environ.get("STREAMVERSE_PROGRESS_FILE")
MIXIN_KEY_ENC_TAB = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35,
    27, 43, 5, 49, 33, 9, 42, 19, 29, 28, 14, 39, 12, 38, 41, 13,
    37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4,
    22, 25, 54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
]

BILIBILI_BATCH_FORMATS = [
    {
        "id": "bestvideo[height<=2160]+bestaudio/best[height<=2160]/best",
        "label": "4K",
        "resolution": "3840x2160",
        "bitrateKbps": 16000,
        "codec": "自适应",
        "container": "MP4",
        "noWatermark": True,
        "requiresLogin": False,
        "requiresProcessing": True,
        "recommended": False,
    },
    {
        "id": "bestvideo[height<=1440]+bestaudio/best[height<=1440]/best",
        "label": "1440P",
        "resolution": "2560x1440",
        "bitrateKbps": 12000,
        "codec": "自适应",
        "container": "MP4",
        "noWatermark": True,
        "requiresLogin": False,
        "requiresProcessing": True,
        "recommended": False,
    },
    {
        "id": "bestvideo[height<=1080]+bestaudio/best[height<=1080]/best",
        "label": "1080P",
        "resolution": "1920x1080",
        "bitrateKbps": 8000,
        "codec": "自适应",
        "container": "MP4",
        "noWatermark": True,
        "requiresLogin": False,
        "requiresProcessing": True,
        "recommended": True,
    },
    {
        "id": "bestvideo[height<=720]+bestaudio/best[height<=720]/best",
        "label": "720P",
        "resolution": "1280x720",
        "bitrateKbps": 4200,
        "codec": "自适应",
        "container": "MP4",
        "noWatermark": True,
        "requiresLogin": False,
        "requiresProcessing": True,
        "recommended": False,
    },
    {
        "id": "bestvideo[height<=480]+bestaudio/best[height<=480]/best",
        "label": "480P",
        "resolution": "854x480",
        "bitrateKbps": 1800,
        "codec": "自适应",
        "container": "MP4",
        "noWatermark": True,
        "requiresLogin": False,
        "requiresProcessing": True,
        "recommended": False,
    },
]


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", required=True)
    parser.add_argument("--cookie-file")
    return parser.parse_args(argv)


def write_progress(current: int, total: int, message: str) -> None:
    if not PROGRESS_FILE:
        return

    path = Path(PROGRESS_FILE)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
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


class ProgressTracker:
    def __init__(self) -> None:
        self.current = 0
        self.regular_total = 0
        self.pending_group_estimate = 0
        self.profile_title = "UP 主"

    def set_profile_title(self, profile_title: str) -> None:
        if profile_title.strip():
            self.profile_title = profile_title.strip()

    def set_regular_total(self, total: int) -> None:
        self.regular_total = max(self.regular_total, int(total or 0))
        self.emit(f"正在读取 {self.profile_title} 的视频列表…")

    def reserve_group_tasks(self, count: int, per_task_estimate: int = 20) -> None:
        if count <= 0:
            return
        self.pending_group_estimate += count * per_task_estimate
        self.emit(f"正在读取 {self.profile_title} 的合集/系列…")

    def reserve_group_items(self, count: int) -> None:
        if count <= 0:
            return
        self.pending_group_estimate += count
        self.emit(f"正在读取 {self.profile_title} 的合集/系列…")

    def advance(self, count: int, message: str | None = None) -> None:
        if count > 0:
            self.current += count
        self.emit(message or f"已解析 {self.current} 个视频。")

    def complete_reserved_group_items(self, count: int) -> None:
        if count > 0:
            self.pending_group_estimate = max(0, self.pending_group_estimate - count)
        self.advance(count, f"已解析 {self.current + max(count, 0)} 个视频。")

    def complete_group_task(self, actual_count: int, per_task_estimate: int = 20) -> None:
        self.pending_group_estimate = max(0, self.pending_group_estimate - per_task_estimate)
        self.advance(actual_count, f"已解析 {self.current + max(actual_count, 0)} 个视频。")

    def total(self) -> int:
        return max(self.current, self.regular_total + self.pending_group_estimate)

    def emit(self, message: str) -> None:
        write_progress(self.current, self.total(), message)

    def finish(self) -> None:
        total = max(self.current, self.regular_total)
        write_progress(total, total, f"主页解析完成，共 {self.current} 个视频。")


def parse_netscape_cookies(cookie_file: str | None) -> list[dict]:
    if not cookie_file:
        return []

    path = Path(cookie_file)
    if not path.exists():
        return []

    cookies: list[dict] = []
    for raw_line in path.read_text("utf-8").splitlines():
        if not raw_line or raw_line.startswith("# "):
            continue
        line = raw_line[len("#HttpOnly_"):] if raw_line.startswith("#HttpOnly_") else raw_line
        parts = line.split("\t")
        if len(parts) < 7:
            continue
        domain, _flag, cookie_path, _secure_flag, _expires, name = parts[:6]
        value = "\t".join(parts[6:])
        cookies.append({
            "domain": domain,
            "path": cookie_path or "/",
            "name": name,
            "value": value,
        })
    return cookies


def cookie_header(cookies: list[dict]) -> str:
    pairs = []
    for cookie in cookies:
        domain = str(cookie.get("domain") or "")
        if not domain or not any(domain.endswith(item) for item in ("bilibili.com", "b23.tv")):
            continue
        name = str(cookie.get("name") or "")
        value = str(cookie.get("value") or "")
        if name:
            pairs.append(f"{name}={value}")
    return "; ".join(pairs)


def has_bilibili_login_cookie(cookies: list[dict]) -> bool:
    login_keys = {"SESSDATA", "DedeUserID", "bili_jct"}
    for cookie in cookies:
        domain = str(cookie.get("domain") or "")
        name = str(cookie.get("name") or "")
        value = str(cookie.get("value") or "")
        if (
            domain
            and any(domain.endswith(item) for item in ("bilibili.com", "b23.tv"))
            and name in login_keys
            and value.strip()
        ):
            return True
    return False


def extract_mid(source_url: str) -> str:
    match = re.search(r"space\.bilibili\.com/(\d+)", source_url)
    if match:
        return match.group(1)
    raise RuntimeError("未能从链接中识别 UP 主 mid，请使用空间页链接。")


def normalize_cover_url(value: str | None) -> str | None:
    if not value:
        return None
    if value.startswith("//"):
        return f"https:{value}"
    return value


def strip_html(text: str | None) -> str:
    value = re.sub(r"<[^>]+>", "", text or "")
    value = re.sub(r"\s+", " ", value).strip()
    return value


def format_timestamp(value) -> str:
    try:
        timestamp = int(float(value or 0))
    except (TypeError, ValueError):
        return "未知"
    if timestamp <= 0:
        return "未知"
    return time.strftime("%Y-%m-%d", time.localtime(timestamp))


def parse_duration_seconds(value) -> int:
    if isinstance(value, (int, float)):
        return max(0, int(value))
    text = str(value or "").strip()
    if text.isdigit():
        return int(text)
    match = re.search(r"(?<!\d)(\d{1,2}):(\d{2})(?::(\d{2}))?(?!\d)", text)
    if not match:
        return 0
    first, second, third = match.groups()
    if third is None:
        return int(first) * 60 + int(second)
    return int(first) * 3600 + int(second) * 60 + int(third)


def normalize_title(value: str, fallback: str) -> str:
    cleaned = strip_html(value)
    return cleaned[:140] if cleaned else fallback


def get_wbi_keys(nav_data: dict) -> tuple[str, str]:
    wbi_img = ((nav_data.get("data") or {}).get("wbi_img") or {})
    img_url = str(wbi_img.get("img_url") or "")
    sub_url = str(wbi_img.get("sub_url") or "")
    if not img_url or not sub_url:
        raise RuntimeError("未能获取 Bilibili WBI 签名参数，请稍后重试。")
    return Path(urlparse(img_url).path).stem, Path(urlparse(sub_url).path).stem


def mixin_key(img_key: str, sub_key: str) -> str:
    source = img_key + sub_key
    return "".join(source[index] for index in MIXIN_KEY_ENC_TAB)[:32]


def sign_wbi_params(params: dict[str, object], img_key: str, sub_key: str) -> dict[str, str]:
    signed = {key: "" if value is None else str(value) for key, value in params.items()}
    signed["wts"] = str(int(time.time()))
    signed = dict(sorted(signed.items()))
    filtered = {
        key: "".join(char for char in value if char not in "!'()*")
        for key, value in signed.items()
    }
    query = urlencode(filtered)
    filtered["w_rid"] = md5(f"{query}{mixin_key(img_key, sub_key)}".encode("utf-8")).hexdigest()
    return filtered


def ensure_api_ok(response: httpx.Response, payload: dict, fallback: str) -> dict:
    code = payload.get("code")
    if response.status_code == 412 or code == -412:
        raise RuntimeError("Bilibili 主页读取被风控拦截。请先在设置中选择已登录的浏览器 Cookie 后再试。")
    if response.status_code >= 400:
        raise RuntimeError(fallback)
    if code not in (0, None):
        message = payload.get("message") or payload.get("msg") or fallback
        raise RuntimeError(str(message))
    data = payload.get("data")
    if not isinstance(data, dict):
        raise RuntimeError(fallback)
    return data


async def fetch_json(
    client: httpx.AsyncClient,
    url: str,
    fallback: str,
    params: dict[str, object] | None = None,
) -> dict:
    response = await client.get(url, params=params)
    try:
        payload = response.json()
    except ValueError as error:
        raise RuntimeError(f"{fallback} 返回了无法解析的响应。") from error
    return ensure_api_ok(response, payload, fallback)


async def gather_limited(values: list[int], worker):
    semaphore = asyncio.Semaphore(FETCH_CONCURRENCY)

    async def run(value: int):
        async with semaphore:
            return await worker(value)

    return await asyncio.gather(*(run(value) for value in values))


def make_vlist_item(item: dict, profile_title: str) -> dict | None:
    bvid = item.get("bvid")
    if not bvid:
        return None
    href = f"https://www.bilibili.com/video/{bvid}"
    return {
        "href": href,
        "title": normalize_title(str(item.get("title") or ""), bvid),
        "author": str(item.get("author") or profile_title or "UP 主"),
        "durationSeconds": parse_duration_seconds(item.get("length") or item.get("duration")),
        "publishDate": format_timestamp(item.get("created") or item.get("pubdate")),
        "coverUrl": normalize_cover_url(item.get("pic")),
        "categoryLabel": "普通视频",
        "groupTitle": None,
    }


def make_group_archive_item(
    item: dict,
    profile_title: str,
    category_label: str,
    group_title: str | None,
) -> dict | None:
    bvid = item.get("bvid") or item.get("bv_id")
    href = item.get("jump_url") or item.get("url")
    if isinstance(href, str) and href.startswith("//"):
        href = f"https:{href}"
    if not href and bvid:
        href = f"https://www.bilibili.com/video/{bvid}"
    if not href:
        return None
    return {
        "href": href,
        "title": normalize_title(str(item.get("title") or ""), bvid or href),
        "author": profile_title,
        "durationSeconds": parse_duration_seconds(item.get("duration") or item.get("length")),
        "publishDate": format_timestamp(item.get("pubdate") or item.get("ctime") or item.get("created")),
        "coverUrl": normalize_cover_url(item.get("pic") or item.get("cover")),
        "categoryLabel": category_label,
        "groupTitle": group_title,
    }


async def fetch_regular_videos(
    client: httpx.AsyncClient,
    mid: str,
    profile_title: str,
    img_key: str,
    sub_key: str,
    tracker: ProgressTracker,
) -> list[dict]:
    items: list[dict] = []
    page_size = 50
    base_params = {
        "mid": mid,
        "ps": page_size,
        "order": "pubdate",
        "tid": 0,
        "keyword": "",
    }

    def map_page_items(vlist: list[dict]) -> list[dict]:
        results = []
        for item in vlist:
            if isinstance(item, dict):
                mapped = make_vlist_item(item, profile_title)
                if mapped:
                    results.append(mapped)
        return results

    first_page = await fetch_json(
        client,
        "https://api.bilibili.com/x/space/wbi/arc/search",
        "读取 Bilibili 投稿列表失败。",
        params=sign_wbi_params({**base_params, "pn": 1}, img_key, sub_key),
    )
    first_vlist = (((first_page.get("list") or {}).get("vlist")) or [])
    if not isinstance(first_vlist, list) or not first_vlist:
        return items

    first_mapped = map_page_items(first_vlist)
    items.extend(first_mapped)
    page_info = first_page.get("page") or {}
    total_count = int(page_info.get("count") or page_info.get("total") or 0)
    total_pages = max(1, (total_count + page_size - 1) // page_size) if total_count else 1
    tracker.set_regular_total(total_count or len(items))
    tracker.advance(len(first_mapped), f"已解析 {len(items)} 个视频。")

    async def fetch_page(page_num: int):
        return await fetch_json(
            client,
            "https://api.bilibili.com/x/space/wbi/arc/search",
            "读取 Bilibili 投稿列表失败。",
            params=sign_wbi_params({**base_params, "pn": page_num}, img_key, sub_key),
        )

    if total_pages > 1:
        for data in await gather_limited(list(range(2, total_pages + 1)), fetch_page):
            vlist = (((data.get("list") or {}).get("vlist")) or [])
            if isinstance(vlist, list):
                mapped = map_page_items(vlist)
                items.extend(mapped)
                tracker.advance(len(mapped), f"已解析 {len(items)} 个视频。")

    return items


async def fetch_group_archives(
    client: httpx.AsyncClient,
    mid: str,
    profile_title: str,
    category_label: str,
    group_title: str | None,
    season_id: int | None = None,
    series_id: int | None = None,
) -> list[dict]:
    params: dict[str, object] = {
        "mid": mid,
        "page_num": 1,
        "page_size": 100,
        "sort_reverse": "false",
    }
    if season_id:
        params["season_id"] = season_id
    if series_id:
        params["series_id"] = series_id

    data = await fetch_json(
        client,
        "https://api.bilibili.com/x/polymer/web-space/seasons_archives_list",
        "读取 Bilibili 合集/系列视频失败。",
        params=params,
    )
    archives = data.get("archives") or data.get("list") or []
    if not isinstance(archives, list):
        return []

    results = []
    for archive in archives:
        if isinstance(archive, dict):
            mapped = make_group_archive_item(archive, profile_title, category_label, group_title)
            if mapped:
                results.append(mapped)
    return results


async def fetch_grouped_videos(
    client: httpx.AsyncClient,
    mid: str,
    profile_title: str,
    tracker: ProgressTracker,
) -> list[dict]:
    items: list[dict] = []
    page_size = 20
    async def fetch_list_page(page_num: int):
        return await fetch_json(
            client,
            "https://api.bilibili.com/x/polymer/web-space/seasons_series_list",
            "读取 Bilibili 合集/系列列表失败。",
            params={
                "mid": mid,
                "page_num": page_num,
                "page_size": page_size,
                "web_location": "333.999",
            },
        )

    def map_archives(archives: list[dict], category_label: str, group_title: str | None) -> list[dict]:
        mapped_items = []
        for archive in archives:
            if isinstance(archive, dict):
                mapped = make_group_archive_item(archive, profile_title, category_label, group_title)
                if mapped:
                    mapped_items.append(mapped)
        return mapped_items

    first_page = await fetch_list_page(1)
    first_lists = first_page.get("items_lists") or {}
    if not isinstance(first_lists, dict):
        return items

    page = first_lists.get("page") or {}
    total = int(page.get("total") or 0)
    total_pages = max(1, (total + page_size - 1) // page_size) if total else 1
    lists_pages = [first_lists]

    if total_pages > 1:
        for data in await gather_limited(list(range(2, total_pages + 1)), fetch_list_page):
            lists = data.get("items_lists") or {}
            if isinstance(lists, dict):
                lists_pages.append(lists)

    inline_archive_total = 0
    deferred_group_total = 0
    group_batches: list[tuple[str, str | None, list[dict]]] = []
    archive_tasks = []
    for lists in lists_pages:
        seasons = lists.get("seasons_list") or []
        series = lists.get("series_list") or []
        for group in seasons:
            if not isinstance(group, dict):
                continue
            meta = group.get("meta") or {}
            group_title = str(meta.get("name") or "").strip() or None
            archives = group.get("archives") or []
            if archives:
                group_batches.append(("合集", group_title, archives))
                inline_archive_total += len(archives)
            elif meta.get("season_id"):
                deferred_group_total += 1
                archive_tasks.append(
                    fetch_group_archives(
                        client,
                        mid,
                        profile_title,
                        "合集",
                        group_title,
                        season_id=int(meta["season_id"]),
                    )
                )

        for group in series:
            if not isinstance(group, dict):
                continue
            meta = group.get("meta") or {}
            group_title = str(meta.get("name") or "").strip() or None
            archives = group.get("archives") or []
            if archives:
                group_batches.append(("系列", group_title, archives))
                inline_archive_total += len(archives)
            elif meta.get("series_id"):
                deferred_group_total += 1
                archive_tasks.append(
                    fetch_group_archives(
                        client,
                        mid,
                        profile_title,
                        "系列",
                        group_title,
                        series_id=int(meta["series_id"]),
                    )
                )

    tracker.reserve_group_items(inline_archive_total)
    tracker.reserve_group_tasks(deferred_group_total)

    for category_label, group_title, archives in group_batches:
        mapped = map_archives(archives, category_label, group_title)
        items.extend(mapped)
        tracker.complete_reserved_group_items(len(mapped))

    if archive_tasks:
        for archives in await asyncio.gather(*archive_tasks):
            items.extend(archives)
            tracker.complete_group_task(len(archives))

    return items


def merge_items(*groups: list[dict]) -> list[dict]:
    merged: dict[str, dict] = {}
    for group in groups:
        for item in group:
            href = str(item.get("href") or "").strip()
            if not href:
                continue
            existing = merged.get(href)
            if not existing:
                merged[href] = item
                continue
            merged[href] = {
                **existing,
                **item,
                "categoryLabel": item.get("categoryLabel") or existing.get("categoryLabel"),
                "groupTitle": item.get("groupTitle") or existing.get("groupTitle"),
                "publishDate": item.get("publishDate") or existing.get("publishDate"),
                "durationSeconds": item.get("durationSeconds") or existing.get("durationSeconds"),
                "coverUrl": item.get("coverUrl") or existing.get("coverUrl"),
            }

    items = list(merged.values())

    def sort_key(item: dict) -> tuple[int, int]:
        publish_date = str(item.get("publishDate") or "")
        if publish_date and publish_date != "未知":
            return (0, -int(publish_date.replace("-", "")))
        return (1, 0)

    return sorted(items, key=sort_key)


def to_asset(raw: dict, profile_title: str) -> dict:
    href = str(raw.get("href") or "")
    asset_id = re.sub(r"^https?://", "", href).split("/")[-1].split("?")[0] or md5(href.encode()).hexdigest()[:16]
    return {
        "assetId": asset_id,
        "platform": "bilibili",
        "sourceUrl": href,
        "title": raw.get("title") or asset_id,
        "author": raw.get("author") or profile_title,
        "durationSeconds": int(raw.get("durationSeconds") or 0),
        "publishDate": raw.get("publishDate") or "未知",
        "caption": "",
        "categoryLabel": raw.get("categoryLabel"),
        "groupTitle": raw.get("groupTitle"),
        "coverUrl": raw.get("coverUrl"),
        "coverGradient": DEFAULT_GRADIENT,
        "formats": [dict(item) for item in BILIBILI_BATCH_FORMATS],
    }


async def main_async(args: argparse.Namespace) -> int:
    cookies = parse_netscape_cookies(args.cookie_file)
    cookie_value = cookie_header(cookies)
    if not cookie_value or not has_bilibili_login_cookie(cookies):
        raise RuntimeError("当前保存的 Cookie 里没有有效的 Bilibili 登录态。请在设置中重新导入已登录 Bilibili 的 Cookie，或直接选择已登录浏览器。")

    mid = extract_mid(args.url)
    tracker = ProgressTracker()
    write_progress(0, 0, "正在读取 Bilibili 主页视频…")
    headers = {
        "user-agent": USER_AGENT,
        "referer": args.url,
        "origin": "https://space.bilibili.com",
        "accept": "application/json, text/plain, */*",
        "accept-language": "zh-CN,zh;q=0.9,en;q=0.8",
        "cookie": cookie_value,
    }

    async with httpx.AsyncClient(headers=headers, follow_redirects=True, timeout=20.0) as client:
        nav_response = await client.get("https://api.bilibili.com/x/web-interface/nav")
        nav_payload = nav_response.json()
        nav_data = ensure_api_ok(nav_response, nav_payload, "读取 Bilibili 导航信息失败。")
        if not bool(nav_data.get("isLogin")):
            raise RuntimeError("当前 Bilibili 登录态已失效。请在设置中重新导入已登录 Bilibili 的 Cookie，或直接选择已登录浏览器。")
        img_key, sub_key = get_wbi_keys({"data": nav_data})

        profile_data_raw = await fetch_json(
            client,
            "https://api.bilibili.com/x/space/wbi/acc/info",
            "读取 UP 主信息失败。",
            params=sign_wbi_params({"mid": mid}, img_key, sub_key),
        )

        profile_title = str(profile_data_raw.get("name") or "UP 主").strip() or "UP 主"
        tracker.set_profile_title(profile_title)
        tracker.emit(f"正在读取 {profile_title} 的视频列表…")

        regular_items = await fetch_regular_videos(
            client, mid, profile_title, img_key, sub_key, tracker
        )
        merged = merge_items(regular_items)
        tracker.current = len(merged)
        tracker.finish()

    if not merged:
        raise RuntimeError("没有读取到可用视频，请确认空间页有效且浏览器登录态可用。")

    payload = {
        "profileTitle": profile_title,
        "sourceUrl": args.url,
        "totalAvailable": len(merged),
        "fetchedCount": len(merged),
        "skippedCount": 0,
        "sessionCookieFile": args.cookie_file,
        "items": [to_asset(item, profile_title) for item in merged],
    }
    print(json.dumps(payload, ensure_ascii=False))
    return 0


def main(argv: list[str]) -> int:
    try:
        args = parse_args(argv)
        import asyncio

        return asyncio.run(main_async(args))
    except Exception as error:  # noqa: BLE001
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
