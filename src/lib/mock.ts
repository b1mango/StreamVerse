import type { BootstrapState } from "./types";

export const mockState: BootstrapState = {
  authState: "guest",
  accountLabel: "未登录",
  isWindows: true,
  platformAuth: {
    douyin: {
      authState: "guest",
      accountLabel: "未登录",
      cookieBrowser: null,
      cookieFile: null
    },
    bilibili: {
      authState: "guest",
      accountLabel: "未登录",
      cookieBrowser: null,
      cookieFile: null
    },
    youtube: {
      authState: "guest",
      accountLabel: "未登录",
      cookieBrowser: null,
      cookieFile: null
    }
  },
  saveDirectory: "~/Movies/StreamVerse",
  downloadMode: "manual",
  qualityPreference: "recommended",
  autoRevealInFinder: false,
  maxConcurrentDownloads: 3,
  proxyUrl: null,
  speedLimit: null,
  autoUpdate: false,
  theme: "dark",
  notifyOnComplete: true,
  language: "zh-CN",
  ffmpegAvailable: true,
  metrics: {
    todayDownloads: 18,
    successRate: "98.4%",
    availableFormats: 6,
    maxQuality: "1080P"
  },
  modules: [
    {
      id: "douyin-single",
      installed: true,
      enabled: true,
      packId: "douyin-pack",
      currentVersion: null,
      latestVersion: "0.1.0",
      sizeBytes: 0,
      sourceKind: "localBuild",
      updateAvailable: false
    },
    {
      id: "douyin-profile",
      installed: true,
      enabled: true,
      packId: "douyin-pack",
      currentVersion: null,
      latestVersion: "0.1.0",
      sizeBytes: 0,
      sourceKind: "localBuild",
      updateAvailable: false
    },
    {
      id: "bilibili-single",
      installed: true,
      enabled: true,
      packId: "bilibili-pack",
      currentVersion: null,
      latestVersion: "0.1.0",
      sizeBytes: 0,
      sourceKind: "localBuild",
      updateAvailable: false
    },
    {
      id: "bilibili-profile",
      installed: true,
      enabled: true,
      packId: "bilibili-pack",
      currentVersion: null,
      latestVersion: "0.1.0",
      sizeBytes: 0,
      sourceKind: "localBuild",
      updateAvailable: false
    },
    {
      id: "youtube-single",
      installed: true,
      enabled: true,
      packId: "youtube-pack",
      currentVersion: null,
      latestVersion: "0.1.0",
      sizeBytes: 0,
      sourceKind: "localBuild",
      updateAvailable: false
    }
  ],
  preview: {
    assetId: "7481035099182375478",
    platform: "douyin",
    sourceUrl: "https://v.douyin.com/XXXXXX/",
    title: "春夜街景的风从镜头里吹过",
    author: "镜头笔记",
    durationSeconds: 42,
    publishDate: "2026-03-28",
    caption: "支持分享文本、短链与作品链接解析。",
    coverUrl: null,
    coverGradient:
      "linear-gradient(135deg, rgba(13, 190, 165, 0.95), rgba(97, 87, 255, 0.8))",
    formats: [
      {
        id: "fhd_nowm",
        label: "1080P",
        resolution: "1920x1080",
        bitrateKbps: 4200,
        codec: "H.264",
        container: "MP4",
        noWatermark: false,
        requiresLogin: false,
        requiresProcessing: false,
        recommended: true
      },
      {
        id: "hd_nowm",
        label: "720P",
        resolution: "1280x720",
        bitrateKbps: 2600,
        codec: "H.264",
        container: "MP4",
        noWatermark: false,
        requiresLogin: false,
        requiresProcessing: false
      },
      {
        id: "fhd_plus",
        label: "1080P 高码率",
        resolution: "1920x1080",
        bitrateKbps: 6200,
        codec: "H.265",
        container: "MP4",
        noWatermark: true,
        requiresLogin: true,
        requiresProcessing: false
      },
      {
        id: "uhd_plus",
        label: "2K 超清",
        resolution: "2560x1440",
        bitrateKbps: 9100,
        codec: "H.265",
        container: "MP4",
        noWatermark: true,
        requiresLogin: true,
        requiresProcessing: false
      }
    ]
  },
  tasks: []
};
