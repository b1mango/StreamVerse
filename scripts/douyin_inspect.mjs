import fs from "node:fs";
import fsp from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { chromium } from "playwright-core";

function parseArgs(argv) {
  const result = {
    url: "",
    cookies: ""
  };

  for (let index = 2; index < argv.length; index += 1) {
    const current = argv[index];
    if (current === "--url") {
      result.url = argv[index + 1] ?? "";
      index += 1;
    } else if (current === "--cookies") {
      result.cookies = argv[index + 1] ?? "";
      index += 1;
    }
  }

  return result;
}

function normalizeExpires(rawValue) {
  const parsed = Number(rawValue);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return undefined;
  }

  if (parsed > 10_000_000_000_000) {
    const unixSeconds = Math.floor(parsed / 1_000_000 - 11_644_473_600);
    return unixSeconds > 0 ? unixSeconds : undefined;
  }

  if (parsed > 253_402_300_799) {
    return undefined;
  }

  return Math.floor(parsed);
}

async function parseNetscapeCookies(filePath) {
  if (!filePath) {
    return [];
  }

  const raw = await fsp.readFile(filePath, "utf8");
  return raw
    .split("\n")
    .filter((line) => line.trim() && !line.startsWith("# "))
    .map((line) => {
      const normalizedLine = line.startsWith("#HttpOnly_")
        ? line.slice("#HttpOnly_".length)
        : line;
      const parts = normalizedLine.split("\t");
      const [domain, _includeSubdomains, cookiePath, secureFlag, expires, name] =
        parts;
      const value = parts.slice(6).join("\t");

      const hostOnly = !domain.startsWith(".");

      return {
        name,
        value,
        domain: hostOnly ? domain : domain.slice(1),
        path: cookiePath || "/",
        secure: secureFlag === "TRUE",
        expires: normalizeExpires(expires)
      };
    })
    .filter((item) => item.domain && item.name && item.value !== undefined);
}

function findChromeExecutable() {
  const candidates = [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
    path.join(
      process.env.HOME ?? "",
      "Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
    )
  ];

  return candidates.find((item) => item && requireFsExists(item));
}

function requireFsExists(filePath) {
  try {
    return Boolean(filePath) && fs.existsSync(filePath);
  } catch {
    return false;
  }
}

async function main() {
  const { url, cookies } = parseArgs(process.argv);
  if (!url) {
    throw new Error("Missing --url");
  }

  const executablePath = findChromeExecutable();
  if (!executablePath) {
    throw new Error("Google Chrome executable not found");
  }

  const browser = await chromium.launch({
    executablePath,
    headless: true
  });

  const context = await browser.newContext({
    userAgent:
      "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/137.0.0.0 Safari/537.36",
    viewport: { width: 1440, height: 960 }
  });

  const cookieList = await parseNetscapeCookies(cookies);
  if (cookieList.length) {
    await context.addCookies(cookieList);
  }

  const page = await context.newPage();
  const responses = [];

  page.on("response", async (response) => {
    const responseUrl = response.url();
    if (
      /douyin\.com/.test(responseUrl) &&
      /(aweme|note|detail|item|post|web\/api)/i.test(responseUrl)
    ) {
      responses.push({
        url: responseUrl,
        status: response.status(),
        contentType: response.headers()["content-type"] ?? ""
      });
    }
  });

  await page.goto(url, {
    waitUntil: "domcontentloaded",
    timeout: 45000
  });

  await page.waitForTimeout(5000);

  const result = await page.evaluate(() => {
    const text = document.body?.innerText ?? "";
    const html = document.documentElement?.outerHTML ?? "";
    const images = Array.from(document.images)
      .map((img) => img.currentSrc || img.src)
      .filter(Boolean)
      .slice(0, 30);
    const videos = Array.from(document.querySelectorAll("video"))
      .map((video) => video.currentSrc || video.src)
      .filter(Boolean);

    return {
      finalUrl: location.href,
      title: document.title,
      hasNotePath: location.pathname.startsWith("/note/"),
      hasVideoPath: location.pathname.startsWith("/video/"),
      bodyTextPreview: text.slice(0, 1200),
      htmlHasRenderData: /RENDER_DATA|SSR_RENDER_DATA/.test(html),
      images,
      videos
    };
  });

  console.log(
    JSON.stringify(
      {
        ...result,
        responses
      },
      null,
      2
    )
  );

  await browser.close();
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
