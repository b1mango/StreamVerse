# Route B Modern Profile Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor StreamVerse profile reading into a modern incremental architecture that shows profile results quickly while progressively enriching thumbnails, metadata, and available formats.

**Architecture:** Keep Tauri 2 + Svelte 5, but split profile work into typed sessions, streaming events, frontend session state, and focused components. The first implementation version preserves existing pack binaries and download flow, then adds streaming session boundaries and enrichment hooks so Douyin, Bilibili, and YouTube profile pages can be optimized without rewriting every extractor at once.

**Tech Stack:** Tauri 2 commands and `Channel`, Rust domain/application modules, Svelte 5 state modules, Vite, yt-dlp, Playwright helper scripts, TanStack Virtual-ready UI boundary.

---

## File Structure

- Create `src-tauri/src/profile_session.rs`
  - Defines `ProfileSessionRequest`, `ProfileSessionEvent`, `ProfileItemPatch`, `ProfileItemStatus`, and helpers to convert current `ProfileBatch` into ordered stream events.
- Modify `src-tauri/src/main.rs`
  - Adds `analyze_profile_stream` command using `tauri::ipc::Channel`.
  - Keeps existing `analyze_profile_input` command for compatibility.
- Modify `src-tauri/src/media_contract.rs`
  - Adds optional status fields for future progressive enrichment without breaking older pack output.
- Create `src/lib/profile-session.svelte.ts`
  - Holds profile session state outside `App.svelte`.
  - Applies append/patch/complete/failure events by `assetId`.
- Modify `src/lib/backend.ts`
  - Adds `analyzeProfileStream`.
  - Keeps existing `analyzeProfileInput`.
- Modify `src/lib/types.ts`
  - Adds stream event and profile item state types.
- Modify `src/lib/components/ProfileBatchWorkspace.svelte`
  - Continues to render `ProfileBatch`, but now supports item status labels and enrichment status.
  - Keeps the recent thumbnail lazy-load behavior.
- Modify `src/App.svelte`
  - Replaces profile analyze handlers with stream-aware wrapper while preserving old fallback behavior.
- Add focused tests in Rust:
  - `profile_session::tests::streams_batch_in_order`
  - `profile_session::tests::patches_include_thumbnail_and_format_status`
- Verification:
  - `npm.cmd run check`
  - `npm.cmd run build`
  - `$env:STREAMVERSE_YTDLP_PATH='C:\Users\b1mango\AppData\Local\Programs\Python\Python310\Scripts\yt-dlp.exe'; cargo test --locked`
  - `$env:STREAMVERSE_YTDLP_PATH='C:\Users\b1mango\AppData\Local\Programs\Python\Python310\Scripts\yt-dlp.exe'; npm.cmd run tauri:build -- --bundles nsis`

---

## Task 1: Profile Session Contract

**Files:**
- Create: `src-tauri/src/profile_session.rs`
- Modify: `src-tauri/src/main.rs`
- Test: `src-tauri/src/profile_session.rs`

- [ ] **Step 1: Write failing Rust tests**

Add tests for converting a `ProfileBatch` to stream events. The expected failure before implementation is unresolved `profile_session` module/types.

- [ ] **Step 2: Implement stream event model**

Add `ProfileSessionEvent` with variants:
- `started`
- `itemsAppended`
- `itemPatched`
- `completed`
- `failed`

The event payload must be camelCase to match frontend types.

- [ ] **Step 3: Register module**

Add `mod profile_session;` in `src-tauri/src/main.rs`.

- [ ] **Step 4: Run Rust tests**

Run:
`$env:STREAMVERSE_YTDLP_PATH='C:\Users\b1mango\AppData\Local\Programs\Python\Python310\Scripts\yt-dlp.exe'; cargo test --locked profile_session -- --nocapture`

Expected: profile session tests pass.

---

## Task 2: Tauri Streaming Command

**Files:**
- Modify: `src-tauri/src/main.rs`
- Modify: `src/lib/backend.ts`
- Modify: `src/lib/types.ts`

- [ ] **Step 1: Add frontend types**

Add TypeScript types matching Rust event names and payloads.

- [ ] **Step 2: Add backend API wrapper**

Add `analyzeProfileStream(payload, onEvent)` using Tauri `Channel`.

- [ ] **Step 3: Add Rust command**

Add `analyze_profile_stream` command. It calls the existing profile analyzer in a blocking task, then sends deterministic session events through the channel.

- [ ] **Step 4: Register command**

Add command to `tauri::generate_handler!`.

- [ ] **Step 5: Run checks**

Run:
`npm.cmd run check`
and
`cargo test --locked profile_session -- --nocapture`

Expected: no TypeScript errors and profile session tests pass.

---

## Task 3: Frontend Session State

**Files:**
- Create: `src/lib/profile-session.svelte.ts`
- Modify: `src/App.svelte`

- [ ] **Step 1: Create session state module**

Implement a small state object with:
- `profileBatch`
- `selectedIds`
- `selectedFormatIds`
- `statusByAssetId`
- `applyEvent(event)`
- `reset()`

- [ ] **Step 2: Wire App analyze handlers**

Use stream command for profile analysis. If a runtime does not support Tauri Channel, use existing `analyzeProfileInput` fallback.

- [ ] **Step 3: Keep platform behavior stable**

Douyin and Bilibili profile buttons, selection, enqueue, and close behavior must keep their existing UX.

- [ ] **Step 4: Run Svelte check**

Run:
`npm.cmd run check`

Expected: 0 errors, 0 warnings.

---

## Task 4: UI Progressive Status

**Files:**
- Modify: `src/lib/components/ProfileBatchWorkspace.svelte`
- Modify: `src/app.css`
- Modify: `src/lib/locales/zh-CN.json`
- Modify: `src/lib/locales/en.json`

- [ ] **Step 1: Render item enrichment state**

Rows should show compact status text:
- metadata ready
- thumbnail loading/ready/failed
- formats pending/loading/ready/failed

- [ ] **Step 2: Keep fast rendering**

Maintain lazy thumbnails and initial visible item count. Do not regress the current first-screen list behavior.

- [ ] **Step 3: Add restrained styles**

Use compact tags, not large decorative cards.

- [ ] **Step 4: Run frontend build**

Run:
`npm.cmd run check`
and
`npm.cmd run build`

Expected: both pass.

---

## Task 5: Enrichment Queue First Pass

**Files:**
- Modify: `src-tauri/src/profile_session.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add deterministic item patches**

After `itemsAppended`, emit `itemPatched` events marking thumbnails and formats as ready or pending based on current item data.

- [ ] **Step 2: Preserve download semantics**

Do not cache direct URLs as permanent data. Existing download task creation must still refresh missing formats at enqueue time.

- [ ] **Step 3: Run Rust tests**

Run:
`cargo test --locked profile_session -- --nocapture`

Expected: patch tests pass.

---

## Task 6: Full Verification and Testable App

**Files:**
- No new source changes expected unless verification exposes failures.

- [ ] **Step 1: Full frontend check**

Run:
`npm.cmd run check`

Expected: `svelte-check found 0 errors and 0 warnings`.

- [ ] **Step 2: Production frontend build**

Run:
`npm.cmd run build`

Expected: Vite build exits 0.

- [ ] **Step 3: Full Rust tests**

Run:
`$env:STREAMVERSE_YTDLP_PATH='C:\Users\b1mango\AppData\Local\Programs\Python\Python310\Scripts\yt-dlp.exe'; cargo test --locked`

Expected: all Rust tests pass.

- [ ] **Step 4: Build installer**

Run:
`$env:STREAMVERSE_YTDLP_PATH='C:\Users\b1mango\AppData\Local\Programs\Python\Python310\Scripts\yt-dlp.exe'; npm.cmd run tauri:build -- --bundles nsis`

Expected installer:
`E:\StreamVerse\src-tauri\target\release\bundle\nsis\StreamVerse_0.1.5_x64-setup.exe`

---

## Self-Review

- The plan covers route B foundation: module boundaries, stream session, frontend state extraction, progressive row status, verification, and installer delivery.
- SQLite cache and YouTube Data API integration are intentionally deferred to the next route B sub-plan because they add credentials, schema migrations, and quota semantics. This plan creates the stream/session boundary they need.
- No task depends on an undefined command without defining it first.
- No task changes download direct URL caching semantics.
