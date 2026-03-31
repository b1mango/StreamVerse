#!/usr/bin/env python3
from __future__ import annotations

import argparse
import asyncio
import hashlib
import json
import os
import re
import socket
import subprocess
import sys
import time
from pathlib import Path
from urllib import request
from urllib.parse import parse_qs, urlparse

from playwright.async_api import async_playwright

DEFAULT_GRADIENT = "linear-gradient(135deg, rgba(13, 190, 165, 0.95), rgba(97, 87, 255, 0.8))"
COLLECT_SCRIPT = """
(platform) => {
  const rules = {
    douyin: /https?:\\/\\/(www\\.)?douyin\\.com\\/(video|note)\\//i,
    bilibili: /https?:\\/\\/(www\\.)?bilibili\\.com\\/video\\//i,
    youtube: /https?:\\/\\/(www\\.)?(youtube\\.com\\/watch\\?v=|youtu\\.be\\/)/i
  };
  const matcher = rules[platform];
  const clean = (value) =>
    (value || "")
      .replace(/\\s+/g, " ")
      .replace(/^[\\s\\-·•]+|[\\s\\-·•]+$/g, "")
      .trim();
  const pickTitle = (anchor, card) => {
    const values = [
      anchor.getAttribute("title"),
      anchor.getAttribute("aria-label"),
      anchor.querySelector("img")?.getAttribute("alt"),
      card?.querySelector("[title]")?.getAttribute("title"),
      anchor.innerText,
      card?.innerText
    ];
    for (const value of values) {
      const cleaned = clean(value);
      if (cleaned && cleaned.length > 1) {
        return cleaned.split("\\n")[0].slice(0, 140);
      }
    }
    return "";
  };
  const cards = Array.from(document.querySelectorAll("a[href]"));
  const items = [];
  const seen = new Set();
  for (const anchor of cards) {
    const href = anchor.href || "";
    if (!matcher.test(href) || seen.has(href)) {
      continue;
    }
    seen.add(href);
    const card = anchor.closest("article, li, section, div");
    items.push({
      href,
      title: pickTitle(anchor, card),
      metaText: clean(card?.innerText || "").slice(0, 260),
      categoryLabel: platform === "bilibili" ? "普通视频" : null,
      groupTitle: null,
      coverUrl:
        anchor.querySelector("img")?.currentSrc ||
        anchor.querySelector("img")?.src ||
        card?.querySelector("img")?.currentSrc ||
        card?.querySelector("img")?.src ||
        null
    });
  }
  return items;
}
"""
FETCH_JSON_SCRIPT = """
async ({ url }) => {
  const response = await fetch(url, {
    credentials: "include",
    headers: {
      accept: "application/json, text/plain, */*"
    }
  });
  const text = await response.text();
  try {
    return {
      ok: response.ok,
      status: response.status,
      body: JSON.parse(text)
    };
  } catch {
    return {
      ok: response.ok,
      status: response.status,
      body: null
    };
  }
}
"""
BILIBILI_SPACE_META_SCRIPT = """
() => {
  const pathname = window.location.pathname || "";
  const urlMatch = pathname.match(/\\/(\\d+)(?:\\/|$)/);
  const initialState = window.__INITIAL_STATE__ || window.__pinia?.state?.value || {};
  const owner = initialState?.space?.info || initialState?.spaceInfo || initialState?.upData || {};
  const titleNode = document.querySelector("h1");
  return {
    mid:
      owner?.mid?.toString?.() ||
      owner?.uid?.toString?.() ||
      initialState?.mid?.toString?.() ||
      (urlMatch ? urlMatch[1] : ""),
    profileTitle:
      titleNode?.textContent?.trim?.() ||
      owner?.name ||
      document.title?.split("_")[0]?.split("-")[0]?.trim?.() ||
      "UP 主"
  };
}
"""
SCROLL_SCRIPT = """
() => {
  const touched = new Set();
  const candidates = [document.scrollingElement, document.documentElement, document.body];
  for (const node of Array.from(document.querySelectorAll("*"))) {
    if (node.scrollHeight - node.clientHeight > 320) {
      candidates.push(node);
    }
  }
  for (const node of candidates) {
    if (!node || touched.has(node)) continue;
    touched.add(node);
    node.scrollTop = node.scrollHeight;
  }
  window.scrollTo(0, document.body.scrollHeight);
}
"""
PROGRESS_FILE = os.environ.get("STREAMVERSE_PROGRESS_FILE")


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--platform", required=True, choices=["douyin", "bilibili", "youtube"])
    parser.add_argument("--url", required=True)
    parser.add_argument("--browser")
    parser.add_argument("--cookie-file")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--launch-manual-browser", action="store_true")
    parser.add_argument("--connect-port", type=int)
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


def state_root() -> Path:
    return Path.home() / ".streamverse" / "browser-reader"


def browser_candidates(preferred: str | None) -> list[tuple[str, list[Path]]]:
    home = Path.home()
    program_files = Path(os.environ.get("PROGRAMFILES", "C:/Program Files"))
    program_files_x86 = Path(os.environ.get("PROGRAMFILES(X86)", "C:/Program Files (x86)"))

    candidates = {
      "chrome": [
        Path("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
        home / "Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        program_files / "Google/Chrome/Application/chrome.exe",
        program_files_x86 / "Google/Chrome/Application/chrome.exe",
      ],
    }

    order = [preferred] if preferred in candidates else []
    order += [name for name in ("chrome",) if name not in order]
    return [(name, candidates[name]) for name in order]


def resolve_browser_executable(preferred: str | None) -> tuple[str, str]:
    for name, paths in browser_candidates(preferred):
        for path in paths:
            if path.exists():
                return name, str(path)
    raise RuntimeError("未找到可用的 Chrome，请先安装 Chrome。")


def browser_state_root() -> Path:
    return state_root() / "profiles"


def allocate_debug_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def wait_for_debug_port(port: int, timeout: float = 15.0) -> None:
    deadline = time.monotonic() + timeout
    url = f"http://127.0.0.1:{port}/json/version"

    while time.monotonic() < deadline:
        try:
            with request.urlopen(url, timeout=1.0) as response:
                if response.status == 200:
                    return
        except Exception:
            time.sleep(0.25)

    raise RuntimeError("浏览器已启动，但调试端口没有就绪，请重试。")


def launch_manual_browser(url: str, preferred: str | None) -> dict:
    browser_name, executable_path = resolve_browser_executable(preferred)
    port = allocate_debug_port()
    user_data_dir = browser_state_root() / f"manual-{browser_name}-{port}"
    user_data_dir.mkdir(parents=True, exist_ok=True)
    command = [
        executable_path,
        f"--remote-debugging-port={port}",
        f"--user-data-dir={user_data_dir}",
        "--new-window",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-blink-features=AutomationControlled",
        url,
    ]

    popen_kwargs = {
        "stdout": subprocess.DEVNULL,
        "stderr": subprocess.DEVNULL,
    }
    if sys.platform == "win32":
        popen_kwargs["creationflags"] = 0x00000008 | 0x00000200
    else:
        popen_kwargs["start_new_session"] = True

    subprocess.Popen(command, **popen_kwargs)
    return {"port": port, "browser": browser_name}


async def parse_netscape_cookies(cookie_file: str | None) -> list[dict]:
    if not cookie_file:
        return []

    path = Path(cookie_file)
    if not path.exists():
        return []

    cookies: list[dict] = []
    for raw_line in path.read_text("utf-8").splitlines():
        if not raw_line or raw_line.startswith("# "):
            continue
        line = raw_line[len("#HttpOnly_") :] if raw_line.startswith("#HttpOnly_") else raw_line
        parts = line.split("\t")
        if len(parts) < 7:
            continue
        domain, _flag, cookie_path, secure_flag, expires, name = parts[:6]
        value = "\t".join(parts[6:])
        normalized_expires = normalize_expires(expires)
        cookie = {
            "name": name,
            "value": value,
            "domain": domain.lstrip("."),
            "path": cookie_path or "/",
            "secure": secure_flag == "TRUE",
        }
        if normalized_expires is not None:
            cookie["expires"] = normalized_expires
        cookies.append(cookie)
    return cookies


def normalize_expires(raw_value: str) -> int | None:
    raw_value = (raw_value or "").strip()
    if not raw_value:
        return -1

    try:
        parsed = int(float(raw_value))
    except ValueError:
        return -1

    if parsed <= 0:
        return -1

    if parsed > 10_000_000_000_000:
        parsed = parsed // 1_000_000 - 11_644_473_600

    if parsed <= 0 or parsed > 253_402_300_799:
        return -1

    return parsed


def normalize_cover_url(value: str | None) -> str | None:
    if not value:
        return None
    if value.startswith("//"):
        return f"https:{value}"
    return value


def cookie_domains(platform: str) -> tuple[str, ...]:
    if platform == "douyin":
        return ("douyin.com", "iesdouyin.com")
    if platform == "bilibili":
        return ("bilibili.com", "b23.tv")
    return ("youtube.com", "youtu.be", "google.com")


def write_cookie_dump(cookies: list[dict], platform: str) -> str:
    target_dir = state_root() / "cookies"
    target_dir.mkdir(parents=True, exist_ok=True)
    target_path = target_dir / f"{platform}.cookies.txt"
    allowed_domains = cookie_domains(platform)
    lines = ["# Netscape HTTP Cookie File"]

    for cookie in cookies:
        domain = str(cookie.get("domain") or "")
        if not domain or not any(domain.endswith(item) for item in allowed_domains):
            continue
        host_only = "FALSE" if domain.startswith(".") else "TRUE"
        secure = "TRUE" if cookie.get("secure") else "FALSE"
        expires = str(int(cookie.get("expires") or 0))
        lines.append(
            "\t".join(
                [
                    domain,
                    host_only,
                    str(cookie.get("path") or "/"),
                    secure,
                    expires,
                    str(cookie.get("name") or ""),
                    str(cookie.get("value") or ""),
                ]
            )
        )

    target_path.write_text("\n".join(lines) + "\n", "utf-8")
    return str(target_path)


def extract_asset_id(platform: str, url: str) -> str:
    parsed = urlparse(url)
    if platform == "douyin":
        match = re.search(r"/(?:video|note)/(\d+)", parsed.path)
        if match:
            return match.group(1)
    elif platform == "bilibili":
        match = re.search(r"/video/([A-Za-z0-9]+)", parsed.path)
        if match:
            return match.group(1)
    else:
        query = parse_qs(parsed.query)
        if "v" in query and query["v"]:
            return query["v"][0]
        parts = [part for part in parsed.path.split("/") if part]
        if parts:
            return parts[-1]

    return hashlib.md5(url.encode("utf-8")).hexdigest()[:16]


def parse_duration_seconds(meta_text: str) -> int:
    match = re.search(r"(?<!\d)(\d{1,2}):(\d{2})(?::(\d{2}))?(?!\d)", meta_text)
    if not match:
        return 0
    first, second, third = match.groups()
    if third is None:
        return int(first) * 60 + int(second)
    return int(first) * 3600 + int(second) * 60 + int(third)


def parse_publish_date(meta_text: str) -> str:
    match = re.search(r"(20\d{2})[./-年](\d{1,2})[./-月](\d{1,2})", meta_text)
    if not match:
        return "未知"
    year, month, day = match.groups()
    return f"{year}-{int(month):02d}-{int(day):02d}"


def normalize_title(title: str, asset_id: str) -> str:
    cleaned = re.sub(r"\s+", " ", (title or "")).strip()
    return cleaned[:140] if cleaned else asset_id


async def collect_items(page, platform: str) -> list[dict]:
    return await evaluate_with_retries(page, COLLECT_SCRIPT, platform)


async def evaluate_with_retries(page, script: str, argument=None, retries: int = 4):
    last_error = None
    for _ in range(retries):
        try:
            if argument is None:
                return await page.evaluate(script)
            return await page.evaluate(script, argument)
        except Exception as error:  # noqa: BLE001
            last_error = error
            if not is_navigation_context_error(error):
                raise
            await page.wait_for_load_state("domcontentloaded")
            await page.wait_for_timeout(260)
    raise last_error


def is_navigation_context_error(error: Exception) -> bool:
    message = str(error)
    return "Execution context was destroyed" in message or "Cannot find context" in message


async def scroll_page(page) -> None:
    await evaluate_with_retries(page, SCROLL_SCRIPT)


async def fetch_json(page, url: str) -> dict | None:
    payload = await evaluate_with_retries(page, FETCH_JSON_SCRIPT, {"url": url})
    if not isinstance(payload, dict):
        return None
    return payload


def ensure_bilibili_payload_ok(payload: dict | None) -> dict | None:
    if not isinstance(payload, dict):
        return None

    status = int(payload.get("status") or 0)
    body = payload.get("body")
    code = body.get("code") if isinstance(body, dict) else None
    if status == 412 or code == -412:
        raise RuntimeError("Bilibili 主页读取被风控拦截。请先在设置中选择已登录的浏览器 Cookie 后再试。")
    return body if isinstance(body, dict) else None


def extract_mid_from_url(url: str) -> str | None:
    match = re.search(r"space\\.bilibili\\.com/(\\d+)", url)
    return match.group(1) if match else None


async def read_bilibili_space_meta(page) -> tuple[str | None, str | None]:
    payload = await evaluate_with_retries(page, BILIBILI_SPACE_META_SCRIPT)
    if not isinstance(payload, dict):
        return None, None
    mid = str(payload.get("mid") or "").strip() or extract_mid_from_url(page.url)
    profile_title = str(payload.get("profileTitle") or "").strip() or None
    return mid or None, profile_title


def format_timestamp(value) -> str:
    try:
        timestamp = int(float(value or 0))
    except (TypeError, ValueError):
        return "未知"
    if timestamp <= 0:
        return "未知"
    return time.strftime("%Y-%m-%d", time.localtime(timestamp))


def coerce_duration(value) -> int:
    if isinstance(value, (int, float)):
        return max(0, int(value))
    if isinstance(value, str):
        text = value.strip()
        if text.isdigit():
            return int(text)
        return parse_duration_seconds(text)
    return 0


def build_bilibili_item(
    archive: dict,
    profile_title: str,
    category_label: str,
    group_title: str | None = None,
) -> dict | None:
    bvid = archive.get("bvid") or archive.get("bv_id") or archive.get("goto_id")
    href = archive.get("jump_url") or archive.get("url")
    if isinstance(href, str) and href.startswith("//"):
        href = f"https:{href}"
    if not href and bvid:
        href = f"https://www.bilibili.com/video/{bvid}"
    if not href:
        return None

    cover_url = normalize_cover_url(
        archive.get("pic")
        or archive.get("cover")
        or archive.get("cover_url")
    )
    title = normalize_title(str(archive.get("title") or ""), extract_asset_id("bilibili", href))
    return {
        "href": href,
        "title": title,
        "metaText": "",
        "coverUrl": cover_url,
        "categoryLabel": category_label,
        "groupTitle": (group_title or "").strip() or None,
        "durationSeconds": coerce_duration(archive.get("duration")),
        "publishDate": format_timestamp(archive.get("pubdate") or archive.get("ctime")),
        "author": profile_title,
    }


async def collect_bilibili_regular_items(page, mid: str, profile_title: str) -> list[dict]:
    items: list[dict] = []
    page_num = 1
    page_size = 50

    while page_num <= 40:
        url = (
            "https://api.bilibili.com/x/series/recArchivesByKeywords"
            f"?mid={mid}&keywords=&pn={page_num}&ps={page_size}&orderby=pubdate"
        )
        payload = await fetch_json(page, url)
        body = ensure_bilibili_payload_ok(payload)
        data = body.get("data") if isinstance(body, dict) else None
        archives = data.get("archives") if isinstance(data, dict) else None

        if not isinstance(archives, list) or not archives:
            break

        for archive in archives:
            if isinstance(archive, dict):
                item = build_bilibili_item(archive, profile_title, "普通视频")
                if item:
                    items.append(item)

        if len(archives) < page_size:
            break
        page_num += 1

    return items


async def collect_bilibili_grouped_items(page, mid: str, profile_title: str) -> list[dict]:
    items: list[dict] = []
    page_num = 1
    page_size = 20

    while page_num <= 20:
        url = (
            "https://api.bilibili.com/x/polymer/web-space/seasons_series_list"
            f"?mid={mid}&page_num={page_num}&page_size={page_size}&web_location=333.999"
        )
        payload = await fetch_json(page, url)
        body = ensure_bilibili_payload_ok(payload)
        data = body.get("data") if isinstance(body, dict) else None
        items_lists = data.get("items_lists") if isinstance(data, dict) else None
        if not isinstance(items_lists, dict):
            break

        seasons = items_lists.get("seasons_list") or []
        series = items_lists.get("series_list") or []
        if not seasons and not series:
            break

        for group in seasons:
            if not isinstance(group, dict):
                continue
            meta = group.get("meta") or {}
            group_title = str(meta.get("name") or "").strip() or None
            for archive in group.get("archives") or []:
                if isinstance(archive, dict):
                    item = build_bilibili_item(archive, profile_title, "合集", group_title)
                    if item:
                        items.append(item)

        for group in series:
            if not isinstance(group, dict):
                continue
            meta = group.get("meta") or {}
            group_title = str(meta.get("name") or "").strip() or None
            for archive in group.get("archives") or []:
                if isinstance(archive, dict):
                    item = build_bilibili_item(archive, profile_title, "系列", group_title)
                    if item:
                        items.append(item)

        page_info = items_lists.get("page") or {}
        total_pages = int(page_info.get("total") or 0)
        if total_pages and page_num >= total_pages:
            break
        page_num += 1

    return items


def merge_profile_items(*groups: list[dict]) -> list[dict]:
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
                "title": item.get("title") or existing.get("title"),
                "coverUrl": item.get("coverUrl") or existing.get("coverUrl"),
                "categoryLabel": item.get("categoryLabel") or existing.get("categoryLabel"),
                "groupTitle": item.get("groupTitle") or existing.get("groupTitle"),
                "durationSeconds": item.get("durationSeconds") or existing.get("durationSeconds"),
                "publishDate": item.get("publishDate") or existing.get("publishDate"),
            }

    items = list(merged.values())

    def sort_key(item: dict) -> tuple[int, int]:
        publish_date = str(item.get("publishDate") or "")
        if publish_date and publish_date != "未知":
            return (0, -int(publish_date.replace("-", "")))
        return (1, 0)

    return sorted(items, key=sort_key)


async def collect_bilibili_profile_items(page) -> tuple[str | None, list[dict]]:
    mid, profile_title = await read_bilibili_space_meta(page)
    if not mid:
        return profile_title, []

    regular_items = await collect_bilibili_regular_items(page, mid, profile_title or "UP 主")
    grouped_items = await collect_bilibili_grouped_items(page, mid, profile_title or "UP 主")
    return profile_title, merge_profile_items(regular_items, grouped_items)


async def wait_until_ready(page, platform: str) -> None:
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        if platform == "bilibili":
            mid, _profile_title = await read_bilibili_space_meta(page)
            if mid:
                return
        items = await collect_items(page, platform)
        if items:
            return
        await page.wait_for_timeout(400)
    raise RuntimeError("浏览器窗口已打开，但还没有读取到作品列表。请先登录并停留在主页视频列表页面。")


async def collect_all_items(page, platform: str) -> list[dict]:
    if platform == "bilibili":
        _profile_title, api_items = await collect_bilibili_profile_items(page)
        if api_items:
            return api_items
        dom_items = await collect_items(page, platform)
        if dom_items:
            return merge_profile_items(dom_items)

    merged: dict[str, dict] = {}
    stagnant_rounds = 0

    for _ in range(220):
        current = await collect_items(page, platform)
        before = len(merged)
        for item in current:
            href = item.get("href") or ""
            if href and href not in merged:
                merged[href] = item

        stagnant_rounds = stagnant_rounds + 1 if len(merged) == before else 0
        if stagnant_rounds >= 10:
            break

        await scroll_page(page)
        await page.wait_for_timeout(360)

    return list(merged.values())


async def read_profile_title(page) -> str:
    title = await page.title()
    if title and title.strip():
        return title.split("_")[0].split("-")[0].strip()

    header = await page.evaluate("() => document.querySelector('h1')?.textContent || ''")
    return (header or "").strip() or "主页"


def run_douyin_profile_bridge(url: str, cookie_file: str) -> dict:
    bridge_script = (
        Path(os.environ["STREAMVERSE_DOUYIN_BRIDGE_PATH"])
        if os.environ.get("STREAMVERSE_DOUYIN_BRIDGE_PATH")
        else Path(__file__).with_name("douyin_bridge.py")
    )
    if not bridge_script.exists():
        raise RuntimeError("未找到抖音主页桥接脚本。")

    write_progress(0, 1, "正在读取抖音主页作品…")
    output = subprocess.run(
        [
            sys.executable,
            str(bridge_script),
            "profile",
            "--url",
            url,
            "--cookie-file",
            cookie_file,
            "--limit",
            "2000",
        ],
        capture_output=True,
        text=True,
    )

    if output.returncode != 0:
        raise RuntimeError(output.stderr.strip() or "读取抖音主页列表失败。")

    try:
        payload = json.loads(output.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError("解析抖音主页桥接结果失败。") from error

    payload["sessionCookieFile"] = cookie_file
    return payload


def page_matches_source(page, source_url: str, platform: str) -> bool:
    current_url = page.url or ""
    if platform == "douyin":
        return "douyin.com" in current_url
    if platform == "bilibili":
        return "bilibili.com" in current_url or "b23.tv" in current_url
    return source_url in current_url


async def connect_existing_browser(playwright, port: int, source_url: str, platform: str):
    last_error = None
    for _ in range(20):
        try:
            browser = await playwright.chromium.connect_over_cdp(f"http://127.0.0.1:{port}")
            break
        except Exception as error:  # noqa: BLE001
            last_error = error
            await asyncio.sleep(0.4)
    else:
        raise last_error or RuntimeError("连接浏览器调试端口失败。")

    if browser.contexts:
        context = browser.contexts[0]
    else:
        context = await browser.new_context()

    candidates = [
        current
        for current in context.pages
        if current.url and current.url not in ("about:blank", "chrome://newtab/")
    ]
    page = next(
        (current for current in reversed(candidates) if page_matches_source(current, source_url, platform)),
        None,
    )
    if page is None:
        page = candidates[-1] if candidates else None
    if page is None:
        page = context.pages[0] if context.pages else await context.new_page()

    return browser, context, page


def to_asset(platform: str, raw: dict, profile_title: str) -> dict:
    url = raw.get("href") or ""
    asset_id = extract_asset_id(platform, url)
    meta_text = raw.get("metaText") or ""
    return {
        "assetId": asset_id,
        "platform": platform,
        "sourceUrl": url,
        "title": normalize_title(raw.get("title") or "", asset_id),
        "author": raw.get("author") or profile_title,
        "durationSeconds": raw.get("durationSeconds") or parse_duration_seconds(meta_text),
        "publishDate": raw.get("publishDate") or parse_publish_date(meta_text),
        "caption": "",
        "categoryLabel": raw.get("categoryLabel"),
        "groupTitle": raw.get("groupTitle"),
        "coverUrl": raw.get("coverUrl"),
        "coverGradient": DEFAULT_GRADIENT,
        "formats": [],
    }


async def async_main(args: argparse.Namespace) -> int:
    if args.launch_manual_browser:
        print(json.dumps(launch_manual_browser(args.url, args.browser), ensure_ascii=False))
        return 0

    browser_name, executable_path = resolve_browser_executable(args.browser)
    imported_cookies = await parse_netscape_cookies(args.cookie_file)
    raw_items: list[dict] = []
    profile_title = "主页"
    exported_cookie_file = ""

    async with async_playwright() as playwright:
        managed_context = False

        if args.connect_port:
            write_progress(0, 1, "正在连接浏览器会话…")
            wait_for_debug_port(args.connect_port, timeout=25.0)
            browser, context, page = await connect_existing_browser(
                playwright,
                args.connect_port,
                args.url,
                args.platform,
            )
        else:
            user_data_dir = browser_state_root() / browser_name
            user_data_dir.mkdir(parents=True, exist_ok=True)
            context = await playwright.chromium.launch_persistent_context(
                str(user_data_dir),
                executable_path=executable_path,
                headless=args.headless,
                viewport={"width": 1480, "height": 960},
                args=["--disable-blink-features=AutomationControlled"],
            )
            browser = None
            page = context.pages[0] if context.pages else await context.new_page()
            managed_context = True

        if imported_cookies:
            try:
                await context.add_cookies(imported_cookies)
            except Exception:
                pass

        should_reload_target = (
            not args.connect_port
            or (args.platform == "douyin" and bool(imported_cookies))
        )
        if should_reload_target:
            write_progress(0, 1, "正在载入主页页面…")
            await page.goto(args.url, wait_until="domcontentloaded", timeout=90_000)
            if args.connect_port:
                await page.wait_for_timeout(1200)
        await page.bring_to_front()
        write_progress(0, 1, "正在读取登录态与页面内容…")
        await wait_until_ready(page, args.platform)
        if args.platform != "douyin":
            raw_items = await collect_all_items(page, args.platform)
            profile_title = await read_profile_title(page)
        exported_cookie_file = write_cookie_dump(await context.cookies(), args.platform)

        if managed_context:
            await context.close()
        elif browser is not None:
            await browser.close()

    if args.platform == "douyin":
        print(json.dumps(run_douyin_profile_bridge(args.url, exported_cookie_file), ensure_ascii=False))
        return 0

    items = [to_asset(args.platform, item, profile_title) for item in raw_items if item.get("href")]
    if not items:
        raise RuntimeError("没有读取到可用作品，请确认已打开正确的主页视频列表页面。")

    payload = {
        "profileTitle": profile_title,
        "sourceUrl": args.url,
        "totalAvailable": len(items),
        "fetchedCount": len(items),
        "skippedCount": 0,
        "sessionCookieFile": exported_cookie_file,
        "items": items,
    }
    print(json.dumps(payload, ensure_ascii=False))
    return 0


def main(argv: list[str]) -> int:
    try:
        args = parse_args(argv)
        return asyncio.run(async_main(args))
    except Exception as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
