export type AuthStatus = "guest" | "active" | "expired" | "needsElevation";
export type AuthMode = "none" | "browser" | "manual";
export type DownloadMode = "manual";
export type PlatformId = "douyin" | "bilibili" | "youtube";
export type ThemeMode = "dark" | "light";
export type LanguageCode = "zh-CN" | "en";
export type QualityPreference = "recommended" | "highest" | "smallest" | "no_watermark";
export type DownloadStatus = "idle" | "analyzing" | "queued" | "downloading" | "paused" | "cancelled" | "completed" | "failed";

export interface DownloadContentSelection {
  downloadVideo: boolean;
  downloadAudio: boolean;
  downloadCover: boolean;
  downloadCaption: boolean;
}

export interface AnalysisProgress {
  current: number;
  total: number;
  message: string;
}

export interface VideoFormat {
  id: string;
  label: string;
  resolution: string;
  bitrateKbps: number;
  codec: string;
  container: string;
  noWatermark: boolean;
  requiresLogin: boolean;
  requiresProcessing: boolean;
  recommended?: boolean;
  directUrl?: string | null;
  referer?: string | null;
  userAgent?: string | null;
  audioDirectUrl?: string | null;
  audioReferer?: string | null;
  audioUserAgent?: string | null;
  fileSizeBytes?: number | null;
}

export interface VideoAsset {
  assetId: string;
  platform: PlatformId;
  sourceUrl: string;
  title: string;
  author: string;
  durationSeconds: number;
  publishDate: string;
  caption: string;
  categoryLabel?: string | null;
  groupTitle?: string | null;
  coverUrl?: string | null;
  coverUrls?: string[];
  coverGradient: string;
  imageUrls?: string[];
  formats: VideoFormat[];
  formatStatus?: "pending" | "loaded" | "failed";
}

export interface DownloadTask {
  id: string;
  platform: PlatformId;
  title: string;
  progress: number;
  speedText: string;
  formatLabel: string;
  status: DownloadStatus;
  etaText: string;
  message?: string;
  outputPath?: string;
  supportsPause: boolean;
  supportsCancel: boolean;
  canRetry: boolean;
  coverUrl?: string | null;
}

export type TaskEvent =
  | { type: "upsert"; task: DownloadTask }
  | { type: "delete"; taskId: string }
  | { type: "reset"; tasks: DownloadTask[] };

export interface PlatformAuthSettings {
  mode: AuthMode;
  browserId?: string | null;
  profileId?: string | null;
  consentedAt?: number | null;
  status: AuthStatus;
}

export interface BrowserProfile {
  id: string;
  label: string;
  isDefault: boolean;
}

export interface BrowserSource {
  id: string;
  label: string;
  isDefault: boolean;
  profiles: BrowserProfile[];
}

export interface CookieImportRequest {
  platform: PlatformId;
  browserId: string;
  profileId?: string | null;
  consent: "once" | "always";
  allowElevation?: boolean;
}

export interface CookieImportResult {
  platform: PlatformId;
  browserId: string;
  profileId?: string | null;
  status: AuthStatus;
  importedCount: number;
  requiresElevation: boolean;
  message: string;
}

export interface AppMetrics {
  todayDownloads: number;
  successRate: string;
  availableFormats: number;
  maxQuality: string;
}

export interface BootstrapState {
  version: string;
  authState: "guest" | "active";
  accountLabel: string;
  isWindows: boolean;
  platformAuth: Record<PlatformId, PlatformAuthSettings>;
  saveDirectory: string;
  downloadMode: DownloadMode;
  qualityPreference: QualityPreference;
  autoRevealInFinder: boolean;
  maxConcurrentDownloads: number;
  proxyUrl: string | null;
  speedLimit: string | null;
  autoUpdate: boolean;
  theme: ThemeMode;
  notifyOnComplete: boolean;
  language: LanguageCode;
  ffmpegAvailable: boolean;
  metrics: AppMetrics;
  preview: VideoAsset;
  tasks: DownloadTask[];
}

export type SettingsProfile = Omit<BootstrapState, "isWindows" | "metrics" | "preview" | "tasks">;

export interface UpdateCheckResult {
  currentVersion: string;
  latestVersion: string;
  hasUpdate: boolean;
  releaseUrl: string;
}

export interface DownloadRequest {
  assetId: string;
  platform: PlatformId;
  sourceUrl: string;
  title: string;
  author: string;
  publishDate: string;
  caption: string;
  coverUrl?: string | null;
  imageUrls?: string[];
  formatId?: string | null;
  formatLabel?: string | null;
  saveDirectory: string;
  downloadOptions: DownloadContentSelection;
  autoRevealInFileManager: boolean;
  directUrl?: string | null;
  referer?: string | null;
  userAgent?: string | null;
  audioDirectUrl?: string | null;
  audioReferer?: string | null;
  audioUserAgent?: string | null;
}

export interface SaveSettingsPayload {
  saveDirectory: string;
  downloadMode: DownloadMode;
  qualityPreference: QualityPreference;
  autoRevealInFinder: boolean;
  maxConcurrentDownloads: number;
  proxyUrl: string | null;
  speedLimit: string | null;
  autoUpdate: boolean;
  theme: ThemeMode;
  notifyOnComplete: boolean;
  language: LanguageCode;
}

export interface BatchItemSelection {
  asset: VideoAsset;
  selectedFormatId?: string | null;
}

export interface ProfileBatch {
  profileTitle: string;
  sourceUrl: string;
  totalAvailable: number;
  fetchedCount: number;
  skippedCount: number;
  items: VideoAsset[];
  /** 前端内部使用：标识一次清晰度补全会话，后端不返回 */
  hydrationKey?: string;
}

export interface BatchDownloadResult {
  profileTitle: string;
  sourceUrl: string;
  totalAvailable: number;
  fetchedCount: number;
  enqueuedCount: number;
  skippedCount: number;
  message: string;
}

export interface DownloadHistoryEntry {
  assetId: string;
  platform: PlatformId;
  title: string;
  downloadedAt: string;
  coverUrl?: string | null;
  outputPath?: string | null;
}
