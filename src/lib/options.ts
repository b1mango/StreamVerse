import type { DownloadStatus, PlatformId, QualityPreference } from "./types";

export const qualityOptions: Array<{ value: QualityPreference; label: string }> = [
  { value: "recommended", label: "推荐优先" },
  { value: "highest", label: "最高质量" },
  { value: "smallest", label: "最小体积" }
];

export const taskLabelMap: Record<DownloadStatus, string> = {
  idle: "空闲",
  analyzing: "解析中",
  queued: "排队中",
  downloading: "下载中",
  paused: "已暂停",
  cancelled: "已取消",
  completed: "已完成",
  failed: "失败"
};

export const platformMeta: Record<PlatformId, { label: string; code: string; color: string }> = {
  douyin: { label: "抖音", code: "DY", color: "var(--douyin)" },
  bilibili: { label: "Bilibili", code: "BL", color: "var(--bilibili)" },
  youtube: { label: "YouTube", code: "YT", color: "var(--youtube)" }
};
