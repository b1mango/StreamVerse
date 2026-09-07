import assert from "node:assert/strict";
import test from "node:test";

import { detectInputTarget, validateInputTarget } from "../../src/lib/input-validation.ts";

test("detects platform and workflow from supported links", () => {
  assert.deepEqual(detectInputTarget("https://space.bilibili.com/123"), {
    platform: "bilibili",
    mode: "profile"
  });
  assert.deepEqual(detectInputTarget("https://www.douyin.com/video/123"), {
    platform: "douyin",
    mode: "unknown"
  });
  assert.deepEqual(detectInputTarget("BV1VPQSBsEdR"), {
    platform: "bilibili",
    mode: "single"
  });
  assert.deepEqual(detectInputTarget("https://www.youtube.com/@stanley1510/featured"), {
    platform: "youtube",
    mode: "profile"
  });
  assert.deepEqual(detectInputTarget("https://www.youtube.com/@stanley1510"), {
    platform: "youtube",
    mode: "profile"
  });
  assert.deepEqual(detectInputTarget("https://www.youtube.com/playlist?list=PL123"), {
    platform: "youtube",
    mode: "playlist"
  });
});

test("rejects a profile URL in a single-video module", () => {
  const message = validateInputTarget(
    "https://space.bilibili.com/123",
    "bilibili",
    "single"
  );
  assert.match(message ?? "", /主页批量/);
});

test("rejects a cross-platform URL with an actionable module name", () => {
  const message = validateInputTarget(
    "https://space.bilibili.com/123",
    "douyin",
    "single"
  );
  assert.match(message ?? "", /Bilibili主页批量/);
});

test("allows ambiguous Douyin short links in either Douyin workflow", () => {
  assert.equal(validateInputTarget("https://v.douyin.com/abcdef/", "douyin", "single"), null);
  assert.equal(validateInputTarget("https://v.douyin.com/abcdef/", "douyin", "profile"), null);
});

test("rejects platform pages that are not a supported video or profile target", () => {
  assert.match(
    validateInputTarget("https://www.bilibili.com/", "bilibili", "single") ?? "",
    /不是当前支持/
  );
  assert.match(validateInputTarget("https://www.youtube.com/@creator", "youtube", "single") ?? "", /主页频道/);
});

test("keeps YouTube channel and playlist workflows separate", () => {
  assert.match(
    validateInputTarget("https://www.youtube.com/@stanley1510", "youtube", "playlist") ?? "",
    /主页频道/
  );
  assert.match(
    validateInputTarget("https://www.youtube.com/playlist?list=PL123", "youtube", "profile") ?? "",
    /合集/
  );
});
