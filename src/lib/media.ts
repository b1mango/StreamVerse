import type {
  AuthState,
  DownloadContentSelection,
  DownloadTask,
  QualityPreference,
  VideoAsset,
  VideoFormat
} from "./types";

export function createDefaultDownloadOptions(): DownloadContentSelection {
  return {
    downloadVideo: true,
    downloadCover: false,
    downloadCaption: false,
    downloadMetadata: false
  };
}

export function hasSelectedDownloadOptions(options: DownloadContentSelection) {
  return (
    options.downloadVideo ||
    options.downloadCover ||
    options.downloadCaption ||
    options.downloadMetadata
  );
}

export function summarizeDownloadOptions(options: DownloadContentSelection) {
  return [
    options.downloadVideo ? "视频" : null,
    options.downloadCover ? "封面" : null,
    options.downloadCaption ? "文案" : null,
    options.downloadMetadata ? "元数据" : null
  ]
    .filter(Boolean)
    .join(" / ");
}

export function visibleFormats(asset: VideoAsset | null, authState: AuthState) {
  const formats = dedupeVisibleFormats(asset?.formats ?? []);
  if (authState === "active") {
    return formats;
  }

  const publicFormats = formats.filter((item) => !item.requiresLogin);
  return publicFormats.length ? publicFormats : formats;
}

export function pickPreferredFormat(
  asset: VideoAsset | null,
  qualityPreference: QualityPreference,
  authState: AuthState
) {
  const candidateFormats = visibleFormats(asset, authState);
  const rankedFormats = [...candidateFormats].sort((left, right) => {
    const heightDelta = formatHeight(right) - formatHeight(left);
    if (heightDelta !== 0) {
      return heightDelta;
    }

    return right.bitrateKbps - left.bitrateKbps;
  });

  switch (qualityPreference) {
    case "highest":
      return rankedFormats[0];
    case "smallest":
      return rankedFormats.at(-1) ?? rankedFormats[0];
    case "no_watermark":
      return (
        rankedFormats.find((item) => item.noWatermark) ??
        rankedFormats.find((item) => item.recommended) ??
        rankedFormats[0]
      );
    case "recommended":
    default:
      return (
        candidateFormats.find((item) => item.recommended) ?? rankedFormats[0]
      );
  }
}

export function selectedFormat(
  asset: VideoAsset | null,
  selectedFormatId: string,
  authState: AuthState
): VideoFormat | undefined {
  return visibleFormats(asset, authState).find((item) => item.id === selectedFormatId);
}

export function formatDuration(totalSeconds: number) {
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes.toString().padStart(2, "0")}:${seconds
    .toString()
    .padStart(2, "0")}`;
}

export function finishedTaskCount(items: DownloadTask[]) {
  return items.filter((task) =>
    ["completed", "failed", "cancelled"].includes(task.status)
  ).length;
}

export function clampBatchLimit(value: number) {
  if (!Number.isFinite(value)) {
    return 24;
  }

  return Math.max(1, Math.min(100, Math.round(value)));
}

export function resolveErrorMessage(error: unknown) {
  const message =
    error instanceof Error ? error.message : typeof error === "string" ? error : "";

  if (
    message.includes("Only Python versions 3.10 and above are supported by yt-dlp") ||
    message.includes("unsupported version of Python")
  ) {
    return "当前应用内置的 yt-dlp 与系统 Python 不兼容，请更新 Python 或重新安装应用后再试。";
  }

  if (message.includes("Traceback")) {
    const importError = message.match(/ImportError:\s*([^\n]+)/);
    if (importError?.[1]) {
      return importError[1].trim();
    }

    const lines = message
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean);
    if (lines.length) {
      return lines.at(-1) ?? "操作失败，请稍后再试。";
    }
  }

  if (error instanceof Error) {
    return error.message;
  }

  if (typeof error === "string") {
    return error;
  }

  return "操作失败，请稍后再试。";
}

function formatHeight(format: VideoFormat) {
  return Number.parseInt(format.resolution.split("x")[1] ?? "0", 10) || 0;
}

function normalizeFormatKey(value: string) {
  return value
    .trim()
    .toUpperCase()
    .replace(/[^A-Z0-9]/g, "");
}

function formatQualityKey(format: VideoFormat) {
  const height = formatHeight(format);
  if (height > 0) {
    return `H${height}`;
  }

  return [normalizeFormatKey(format.label), normalizeFormatKey(format.resolution)].join("|");
}

function dedupeVisibleFormats(formats: VideoFormat[]) {
  const deduped = new Map<string, VideoFormat>();

  for (const format of formats) {
    const key = [formatQualityKey(format), format.requiresLogin ? "LOGIN" : "PUBLIC"].join("|");
    const existing = deduped.get(key);

    if (!existing) {
      deduped.set(key, format);
      continue;
    }

    const shouldReplace =
      (format.recommended && !existing.recommended) ||
      (format.noWatermark && !existing.noWatermark) ||
      (Boolean(format.directUrl) && !existing.directUrl) ||
      (Boolean(format.audioDirectUrl) && !existing.audioDirectUrl) ||
      format.bitrateKbps > existing.bitrateKbps ||
      (format.bitrateKbps === existing.bitrateKbps &&
        codecPriority(format.codec) < codecPriority(existing.codec));

    if (shouldReplace) {
      deduped.set(key, format);
    }
  }

  return Array.from(deduped.values());
}

function codecPriority(value: string) {
  const normalized = normalizeFormatKey(value);
  if (normalized.startsWith("H264")) {
    return 0;
  }
  if (normalized.startsWith("H265") || normalized.startsWith("HEVC")) {
    return 1;
  }
  if (normalized.startsWith("AV1")) {
    return 2;
  }
  if (normalized.startsWith("VP9")) {
    return 3;
  }
  return 4;
}
