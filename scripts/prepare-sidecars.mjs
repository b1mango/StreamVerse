import { createHash } from "node:crypto";
import { createWriteStream, existsSync } from "node:fs";
import { chmod, copyFile, mkdir, readFile, rename, rm } from "node:fs/promises";
import { execFile } from "node:child_process";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const lock = JSON.parse(await readFile(join(root, "sidecars.lock.json"), "utf8"));
const execFileAsync = promisify(execFile);
const platformKey = `${process.platform === "win32" ? "windows" : "macos"}-${process.arch}`;
const triple = {
  "windows-x64": "x86_64-pc-windows-msvc",
  "macos-x64": "x86_64-apple-darwin",
  "macos-arm64": "aarch64-apple-darwin"
}[platformKey];

if (!triple) {
  throw new Error(`Unsupported sidecar target: ${process.platform}/${process.arch}`);
}

const extension = process.platform === "win32" ? ".exe" : "";
const outputDir = join(root, "src-tauri", "binaries");
await mkdir(outputDir, { recursive: true });

async function sha256(path) {
  return createHash("sha256").update(await readFile(path)).digest("hex");
}

async function verify(path, expected, label) {
  const actual = await sha256(path);
  if (actual !== expected) {
    throw new Error(`${label} SHA256 mismatch: expected ${expected}, got ${actual}`);
  }
}

async function download(url, target) {
  const response = await fetch(url, { redirect: "follow" });
  if (!response.ok || !response.body) {
    throw new Error(`Sidecar download failed (${response.status}): ${url}`);
  }
  const temp = `${target}.download`;
  await rm(temp, { force: true });
  const file = createWriteStream(temp);
  await new Promise((resolvePromise, reject) => {
    response.body.pipeTo(new WritableStream({
      write(chunk) { file.write(Buffer.from(chunk)); },
      close() { file.end(resolvePromise); },
      abort(error) { file.destroy(); reject(error); }
    })).catch(reject);
  });
  await rename(temp, target);
}

async function prepareYtDlp() {
  const spec = lock.ytDlp[platformKey];
  const target = join(outputDir, `yt-dlp-${triple}${extension}`);
  const explicit = process.env.STREAMVERSE_YTDLP_PATH;
  const previous = join(root, "src-tauri", "gen", "resources", "download-engine", "bin", `yt-dlp${extension}`);
  if (explicit) {
    await copyFile(explicit, target);
  } else if (existsSync(previous) && await sha256(previous) === spec.sha256) {
    await copyFile(previous, target);
  } else if (!existsSync(target) || await sha256(target) !== spec.sha256) {
    await download(spec.url, target);
  }
  await verify(target, spec.sha256, `yt-dlp ${lock.ytDlp.version}`);
  if (process.platform !== "win32") await chmod(target, 0o755);
}

async function prepareAria2() {
  if (platformKey !== "windows-x64") return;

  const spec = lock.aria2[platformKey];
  const target = join(outputDir, `aria2c-${triple}${extension}`);
  const licenseTarget = join(root, "src-tauri", "third-party", "aria2-COPYING");
  const sourceTarget = join(root, "src-tauri", "third-party", `aria2-${lock.aria2.version}-source.tar.xz`);
  const explicit = process.env.STREAMVERSE_ARIA2_PATH;
  if (explicit) {
    await copyFile(explicit, target);
  } else if (!existsSync(target) || await sha256(target) !== spec.executableSha256 || !existsSync(licenseTarget)) {
    const archive = join(outputDir, `.aria2-${platformKey}.zip`);
    const extractDir = join(outputDir, `.aria2-${platformKey}`);
    if (!existsSync(archive) || await sha256(archive) !== spec.archiveSha256) {
      await download(spec.url, archive);
    }
    await verify(archive, spec.archiveSha256, `aria2 ${lock.aria2.version} archive`);
    await rm(extractDir, { recursive: true, force: true });
    await mkdir(extractDir, { recursive: true });
    await execFileAsync("tar", ["-xf", archive, "-C", extractDir]);
    const packageDir = join(extractDir, `aria2-${lock.aria2.version}-win-64bit-build1`);
    const extracted = join(packageDir, "aria2c.exe");
    await verify(extracted, spec.executableSha256, `aria2 ${lock.aria2.version}`);
    await copyFile(extracted, target);
    await mkdir(dirname(licenseTarget), { recursive: true });
    await copyFile(join(packageDir, "COPYING"), licenseTarget);
    await rm(archive, { force: true });
    await rm(extractDir, { recursive: true, force: true });
  }
  await verify(target, spec.executableSha256, `aria2 ${lock.aria2.version}`);
  if (!existsSync(sourceTarget) || await sha256(sourceTarget) !== lock.aria2.sourceSha256) {
    await download(lock.aria2.sourceUrl, sourceTarget);
  }
  await verify(sourceTarget, lock.aria2.sourceSha256, `aria2 ${lock.aria2.version} source`);
}

async function prepareDeno() {
  const spec = lock.deno[platformKey];
  const target = join(outputDir, `deno-${triple}${extension}`);
  const explicit = process.env.STREAMVERSE_DENO_PATH;
  if (explicit) {
    await copyFile(explicit, target);
    await verify(target, spec.executableSha256, `Deno ${lock.deno.version}`);
  } else if (!existsSync(target) || await sha256(target) !== spec.executableSha256) {
    const archive = join(outputDir, `.deno-${platformKey}.zip`);
    const extractDir = join(outputDir, `.deno-${platformKey}`);
    if (!existsSync(archive) || await sha256(archive) !== spec.archiveSha256) {
      await download(spec.url, archive);
    }
    await verify(archive, spec.archiveSha256, `Deno ${lock.deno.version} archive`);
    await rm(extractDir, { recursive: true, force: true });
    await mkdir(extractDir, { recursive: true });
    await execFileAsync("tar", ["-xf", archive, "-C", extractDir]);
    const extracted = join(extractDir, `deno${extension}`);
    await verify(extracted, spec.executableSha256, `Deno ${lock.deno.version}`);
    await copyFile(extracted, target);
    await rm(archive, { force: true });
    await rm(extractDir, { recursive: true, force: true });
  }
  await verify(target, spec.executableSha256, `Deno ${lock.deno.version}`);
  if (process.platform !== "win32") await chmod(target, 0o755);
}

async function prepareFfmpeg() {
  const require = createRequire(import.meta.url);
  const source = process.env.STREAMVERSE_FFMPEG_PATH || require("ffmpeg-static");
  if (!source || !existsSync(source)) {
    throw new Error("ffmpeg-static binary is missing; run npm ci with install scripts enabled.");
  }
  await verify(source, lock.ffmpeg[platformKey], `FFmpeg ${lock.ffmpeg.version}`);
  const target = join(outputDir, `ffmpeg-${triple}${extension}`);
  await copyFile(source, target);
  if (process.platform !== "win32") await chmod(target, 0o755);
}

async function prepareHelper() {
  const explicit = process.env.STREAMVERSE_HELPER_PATH;
  const source = explicit || join(root, "scripts", "dist", `streamverse-helper${extension}`);
  if (!existsSync(source)) {
    throw new Error("streamverse-helper is missing; run npm run build:helper first.");
  }
  const target = join(outputDir, `streamverse-helper-${triple}${extension}`);
  await copyFile(source, target);
  if (process.platform !== "win32") await chmod(target, 0o755);
}

await prepareYtDlp();
await prepareAria2();
await prepareDeno();
await prepareFfmpeg();
await prepareHelper();
console.log(`Prepared verified sidecars for ${triple}.`);
