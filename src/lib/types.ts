export type AuthState = "guest" | "active" | "expired";
export type DownloadMode = "manual" | "smart";
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
  | "completed"
  | "failed";

export interface VideoFormat {
  id: string;
  label: string;
  resolution: string;
  bitrateKbps: number;
  codec: string;
  container: string;
  noWatermark: boolean;
  requiresLogin: boolean;
  recommended?: boolean;
  directUrl?: string | null;
  referer?: string | null;
  userAgent?: string | null;
}

export interface VideoAsset {
  awemeId: string;
  sourceUrl: string;
  title: string;
  author: string;
  durationSeconds: number;
  publishDate: string;
  caption: string;
  coverGradient: string;
  formats: VideoFormat[];
}

export interface DownloadTask {
  id: string;
  title: string;
  progress: number;
  speedText: string;
  formatLabel: string;
  status: DownloadStatus;
  etaText: string;
  message?: string;
  outputPath?: string;
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
}

export interface AnalyzeInputPayload {
  rawInput: string;
}

export interface CreateTaskPayload {
  awemeId: string;
  sourceUrl: string;
  title: string;
  formatId: string;
  formatLabel: string;
  saveDirectoryOverride?: string | null;
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
  rawInput: string;
  limit?: number;
  saveDirectoryOverride?: string | null;
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
