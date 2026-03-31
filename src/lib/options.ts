import type {
  AuthState,
  DownloadMode,
  DownloadStatus,
  ModuleId,
  PlatformId,
  QualityPreference
} from "./types";

export const browserOptions = [
  { value: "", label: "未登录" },
  { value: "chrome", label: "Chrome" }
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
    description: "单视频 / 主页批量",
    status: "可用",
    accent: "mint"
  },
  bilibili: {
    label: "Bilibili",
    badge: "Bilibili",
    description: "单视频 / UP 主批量",
    status: "可用",
    accent: "blue"
  },
  youtube: {
    label: "YouTube",
    badge: "YouTube",
    description: "单视频下载",
    status: "可用",
    accent: "slate"
  }
};

export const moduleCatalog: Record<
  ModuleId,
  {
    id: ModuleId;
    platform: PlatformId;
    label: string;
    badge: string;
    description: string;
    accent: string;
    dependencyHints: string[];
  }
> = {
  "douyin-single": {
    id: "douyin-single",
    platform: "douyin",
    label: "抖音单视频下载",
    badge: "Douyin",
    description: "分享文案、短链、作品页链接解析与下载。",
    accent: "mint",
    dependencyHints: ["分享链接解析"]
  },
  "douyin-profile": {
    id: "douyin-profile",
    platform: "douyin",
    label: "抖音主页批量下载",
    badge: "Douyin",
    description: "打开浏览器登录后读取主页作品，再勾选入队。",
    accent: "mint",
    dependencyHints: ["浏览器读取"]
  },
  "bilibili-single": {
    id: "bilibili-single",
    platform: "bilibili",
    label: "Bilibili 单视频下载",
    badge: "Bilibili",
    description: "支持单视频格式解析与高质量合流下载。",
    accent: "blue",
    dependencyHints: ["高质量合流"]
  },
  "bilibili-profile": {
    id: "bilibili-profile",
    platform: "bilibili",
    label: "Bilibili 主页批量下载",
    badge: "Bilibili",
    description: "读取 UP 主投稿列表后批量入队。",
    accent: "blue",
    dependencyHints: ["浏览器 Cookie", "批量入队"]
  },
  "youtube-single": {
    id: "youtube-single",
    platform: "youtube",
    label: "YouTube 单视频下载",
    badge: "YouTube",
    description: "解析 YouTube 视频并按所选格式下载。",
    accent: "slate",
    dependencyHints: ["高质量格式"]
  }
};

export const moduleOrder: ModuleId[] = [
  "douyin-single",
  "douyin-profile",
  "bilibili-single",
  "bilibili-profile",
  "youtube-single"
];
