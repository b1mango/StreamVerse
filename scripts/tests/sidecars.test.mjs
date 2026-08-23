import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

test("sidecar lock pins every supported desktop target", async () => {
  const lock = JSON.parse(await readFile(new URL("../../sidecars.lock.json", import.meta.url)));
  for (const target of ["windows-x64", "macos-x64", "macos-arm64"]) {
    assert.match(lock.ytDlp[target].sha256, /^[a-f0-9]{64}$/);
    assert.match(lock.deno[target].archiveSha256, /^[a-f0-9]{64}$/);
    assert.match(lock.deno[target].executableSha256, /^[a-f0-9]{64}$/);
    assert.match(lock.ffmpeg[target], /^[a-f0-9]{64}$/);
  }
  assert.equal(lock.aria2.license, "GPL-2.0-or-later");
  assert.match(lock.aria2.sourceSha256, /^[a-f0-9]{64}$/);
  assert.match(lock.aria2["windows-x64"].archiveSha256, /^[a-f0-9]{64}$/);
  assert.match(lock.aria2["windows-x64"].executableSha256, /^[a-f0-9]{64}$/);
});
