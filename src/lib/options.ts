import type {
  AuthState,
  DownloadMode,
  DownloadStatus,
  PlatformId,
  QualityPreference
} from "./types";

export const browserOptions = [
  { value: "", label: "未登录" },
  { value: "chrome", label: "Chrome" },
  { value: "safari", label: "Safari" },
  { value: "firefox", label: "Firefox" },
  { value: "edge", label: "Edge" },
  { value: "brave", label: "Brave" }
];

export const downloadModeOptions: Array<{ value: DownloadMode; label: string }> = [
  { value: "manual", label: "手动模式" },
  { value: "smart", label: "智能模式" }
];

export const qualityOptions: Array<{ value: QualityPreference; label: string }> = [
  { value: "recommended", label: "推荐优先" },
  { value: "highest", label: "最高质量" },
  { value: "no_watermark", label: "无水印优先" },
  { value: "smallest", label: "最小体积" }
];

export const authMap: Record<AuthState, string> = {
  guest: "游客模式",
  active: "已登录",
  expired: "登录失效"
};

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

export const modeLabelMap: Record<DownloadMode, string> = {
  manual: "手动下载",
  smart: "智能下载"
};

export const platformMeta: Record<
  PlatformId,
  {
    label: string;
    badge: string;
    description: string;
    status: string;
    accent: string;
  }
> = {
  douyin: {
    label: "抖音",
    badge: "Douyin",
    description: "单视频下载与主页批量下载都已接入。",
    status: "可用",
    accent: "mint"
  },
  bilibili: {
    label: "Bilibili",
    badge: "Bilibili",
    description: "优先支持单视频下载，高质量格式需要 FFmpeg。",
    status: "Beta",
    accent: "blue"
  },
  youtube: {
    label: "YouTube",
    badge: "YouTube",
    description: "页面结构已预留，后续接入下载能力。",
    status: "即将推出",
    accent: "slate"
  }
};
