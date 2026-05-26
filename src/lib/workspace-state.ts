import type {
  AnalysisProgress,
  DownloadContentSelection,
  PlatformId,
  ProfileBatch,
  VideoAsset,
  BrowserLaunchResult
} from "./types";
import { createDefaultDownloadOptions } from "./media";

/** Per-platform workspace state — replaces 30+ individual variables in App.svelte */
export interface PlatformWorkspaceState {
  // Single video
  singleInput: string;
  singlePreview: VideoAsset | null;
  singleFormatId: string;
  singleOptions: DownloadContentSelection;
  singleAnalysisProgress: AnalysisProgress | null;
  analyzingSingle: boolean;
  downloadingSingle: boolean;
  pastingSingle: boolean;

  // Profile batch
  profileInput: string;
  profilePreview: ProfileBatch | null;
  profileSelectedIds: string[];
  profileFormatIds: Record<string, string>;
  profileOptions: DownloadContentSelection;
  profileAnalysisProgress: AnalysisProgress | null;
  profileBrowserSession: BrowserLaunchResult | null;
  analyzingProfile: boolean;
  openingProfileBrowser: boolean;
  enqueuingProfile: boolean;
  pastingProfile: boolean;
  downloadedAssetIds: string[];
}

export function createPlatformState(): PlatformWorkspaceState {
  return {
    singleInput: "",
    singlePreview: null,
    singleFormatId: "",
    singleOptions: createDefaultDownloadOptions(),
    singleAnalysisProgress: null,
    analyzingSingle: false,
    downloadingSingle: false,
    pastingSingle: false,

    profileInput: "",
    profilePreview: null,
    profileSelectedIds: [],
    profileFormatIds: {},
    profileOptions: createDefaultDownloadOptions(),
    profileAnalysisProgress: null,
    profileBrowserSession: null,
    analyzingProfile: false,
    openingProfileBrowser: false,
    enqueuingProfile: false,
    pastingProfile: false,
    downloadedAssetIds: []
  };
}

/** Create initial states for all platforms */
export function createAllPlatformStates(): Record<PlatformId, PlatformWorkspaceState> {
  return {
    douyin: createPlatformState(),
    bilibili: createPlatformState(),
    youtube: createPlatformState()
  };
}
