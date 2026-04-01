export type AuthState = "guest" | "active" | "expired";
export type DownloadMode = "manual";
export type PlatformId = "douyin" | "bilibili" | "youtube";
export type ThemeMode = "dark" | "light";
export type LanguageCode = "zh-CN" | "en";
export type ModuleId =
  | "douyin-single"
  | "douyin-profile"
  | "bilibili-single"
  | "bilibili-profile"
  | "youtube-single";
export type QualityPreference =
  | "recommended"
  | "highest"
  | "smallest"
  | "no_watermark";

export type DownloadStatus =
  | "idle"
  | "analyzing"
  | "queued"
  | "downloading"
  | "paused"
  | "cancelled"
  | "completed"
  | "failed";

export interface DownloadContentSelection {
  downloadVideo: boolean;
  downloadCover: boolean;
  downloadCaption: boolean;
  downloadMetadata: boolean;
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
  coverGradient: string;
  formats: VideoFormat[];
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
}

export interface AppMetrics {
  todayDownloads: number;
  successRate: string;
  availableFormats: number;
  maxQuality: string;
}

export interface ModuleRuntimeState {
  id: ModuleId;
  installed: boolean;
  enabled: boolean;
  packId?: string | null;
  currentVersion?: string | null;
  latestVersion?: string | null;
  sizeBytes?: number | null;
  sourceKind?: string | null;
  updateAvailable: boolean;
}

export interface ModuleInstallProgress {
  percent: number;
  label: string;
}

export interface BootstrapState {
  authState: AuthState;
  accountLabel: string;
  cookieBrowser: string | null;
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
  modules: ModuleRuntimeState[];
  preview: VideoAsset;
  tasks: DownloadTask[];
}

export interface AuthProfile {
  authState: AuthState;
  accountLabel: string;
  cookieBrowser: string | null;
}

export interface SettingsProfile {
  authState: AuthState;
  accountLabel: string;
  cookieBrowser: string | null;
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
}

export interface AnalyzeInputPayload {
  rawInput: string;
  sessionId?: string | null;
}

export interface CreateTaskPayload {
  assetId: string;
  platform: PlatformId;
  sourceUrl: string;
  title: string;
  author: string;
  publishDate: string;
  caption: string;
  coverUrl?: string | null;
  formatId?: string | null;
  formatLabel?: string | null;
  saveDirectoryOverride?: string | null;
  downloadOptions: DownloadContentSelection;
  directUrl?: string | null;
  referer?: string | null;
  userAgent?: string | null;
  audioDirectUrl?: string | null;
  audioReferer?: string | null;
  audioUserAgent?: string | null;
}

export interface SaveSettingsPayload {
  cookieBrowser: string | null;
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

export interface SetModuleEnabledPayload {
  moduleId: ModuleId;
  enabled: boolean;
}

export interface BatchItemSelection {
  asset: VideoAsset;
  selectedFormatId?: string | null;
}

export interface CreateProfileDownloadTasksPayload {
  profileTitle: string;
  sourceUrl: string;
  items: BatchItemSelection[];
  sessionCookieFile?: string | null;
  saveDirectoryOverride?: string | null;
  downloadOptions: DownloadContentSelection;
}

export interface AnalyzeProfilePayload {
  rawInput: string;
  limit?: number;
  sessionId?: string | null;
}

export interface AnalysisProgress {
  current: number;
  total: number;
  message: string;
}

export interface BrowserLaunchResult {
  port: number;
  browser: string;
}

export interface ProfileBatch {
  profileTitle: string;
  sourceUrl: string;
  totalAvailable: number;
  fetchedCount: number;
  skippedCount: number;
  sessionCookieFile?: string | null;
  items: VideoAsset[];
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
