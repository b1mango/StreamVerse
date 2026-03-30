export type AuthState = "guest" | "active" | "expired";
export type DownloadMode = "manual" | "smart";
export type PlatformId = "douyin" | "bilibili" | "youtube";
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
}

export interface AppMetrics {
  todayDownloads: number;
  successRate: string;
  availableFormats: number;
  maxQuality: string;
}

export interface BootstrapState {
  authState: AuthState;
  accountLabel: string;
  cookieBrowser: string | null;
  saveDirectory: string;
  downloadMode: DownloadMode;
  qualityPreference: QualityPreference;
  autoRevealInFinder: boolean;
  ffmpegAvailable: boolean;
  metrics: AppMetrics;
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
  ffmpegAvailable: boolean;
}

export interface AnalyzeInputPayload {
  rawInput: string;
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
}

export interface SaveSettingsPayload {
  cookieBrowser: string | null;
  saveDirectory: string;
  downloadMode: DownloadMode;
  qualityPreference: QualityPreference;
  autoRevealInFinder: boolean;
}

export interface CreateProfileDownloadTasksPayload {
  profileTitle: string;
  sourceUrl: string;
  items: VideoAsset[];
  saveDirectoryOverride?: string | null;
  downloadOptions: DownloadContentSelection;
}

export interface AnalyzeProfilePayload {
  rawInput: string;
  limit?: number;
}

export interface ProfileBatch {
  profileTitle: string;
  sourceUrl: string;
  totalAvailable: number;
  fetchedCount: number;
  skippedCount: number;
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
