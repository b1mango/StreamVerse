import type {
  AuthStatus,
  DownloadContentSelection,
  DownloadTask,
  QualityPreference,
  VideoAsset,
  VideoFormat
} from "./types";

export function createDefaultDownloadOptions(): DownloadContentSelection {
  return {
    downloadVideo: true,
    downloadAudio: false,
    downloadCover: false,
    downloadCaption: false
  };
}

export function hasSelectedDownloadOptions(options: DownloadContentSelection) {
  return (
    options.downloadVideo ||
    options.downloadAudio ||
    options.downloadCover ||
    options.downloadCaption
  );
}

export function summarizeDownloadOptions(options: DownloadContentSelection) {
  return [
    options.downloadVideo ? "视频" : null,
    options.downloadAudio ? "MP3" : null,
    options.downloadCover ? "封面" : null,
    options.downloadCaption ? "文案" : null
  ]
    .filter(Boolean)
    .join(" / ");
}

export function visibleFormats(asset: VideoAsset | null, authState: AuthStatus) {
  const formats = asset?.formats ?? [];
  const publicFormats = formats.filter((item) => !item.requiresLogin);
  const available = authState === "active" || publicFormats.length === 0 ? formats : publicFormats;
  return dedupeVisibleFormats(available);
}

export function pickPreferredFormat(
  asset: VideoAsset | null,
  qualityPreference: QualityPreference,
  authState: AuthStatus
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
  authState: AuthStatus
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

export function formatFileSize(bytes: number): string {
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(0)} KB`;
  }
  if (bytes < 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function estimateFileSize(bitrateKbps: number, durationSeconds: number): number | null {
  if (!bitrateKbps || !durationSeconds) return null;
  return Math.round((bitrateKbps * 1000 * durationSeconds) / 8);
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
  const labelHeight = format.label.match(/(\d{3,4})\s*[pP]/)?.[1];
  if (labelHeight) return Number.parseInt(labelHeight, 10);

  const dimensions = format.resolution
    .split("x")
    .map((value) => Number.parseInt(value, 10))
    .filter((value) => Number.isFinite(value) && value > 0);
  return dimensions.length >= 2 ? Math.min(...dimensions) : dimensions[0] ?? 0;
}

function normalizeFormatKey(value: string) {
  return value
    .trim()
    .toUpperCase()
    .replace(/[^A-Z0-9]/g, "");
}

function formatQualityKey(format: VideoFormat) {
  const height = formatHeight(format);
  return height > 0
    ? `H${height}`
    : [normalizeFormatKey(format.label), normalizeFormatKey(format.resolution)].join("|");
}

function dedupeVisibleFormats(formats: VideoFormat[]) {
  const deduped = new Map<string, VideoFormat>();

  for (const format of formats) {
    const key = [
      formatQualityKey(format),
      normalizeFormatKey(format.codec),
      normalizeFormatKey(format.container)
    ].join("|");
    const existing = deduped.get(key);

    if (!existing) {
      deduped.set(key, format);
      continue;
    }

    const shouldReplace =
      (Boolean(format.directUrl) && !existing.directUrl) ||
      (Boolean(format.audioDirectUrl) && !existing.audioDirectUrl) ||
      format.bitrateKbps > existing.bitrateKbps ||
      (format.bitrateKbps === existing.bitrateKbps && format.recommended && !existing.recommended) ||
      (format.bitrateKbps === existing.bitrateKbps && format.noWatermark && !existing.noWatermark);

    if (shouldReplace) {
      deduped.set(key, format);
    }
  }

  return Array.from(deduped.values()).sort((left, right) =>
    formatHeight(right) - formatHeight(left) ||
    codecPriority(left.codec) - codecPriority(right.codec) ||
    right.bitrateKbps - left.bitrateKbps
  );
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
