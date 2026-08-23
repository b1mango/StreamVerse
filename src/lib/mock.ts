import type { BootstrapState } from "./types";

export const mockState: BootstrapState = {
  version: "1.0.0",
  authState: "guest",
  accountLabel: "No active sessions",
  isWindows: true,
  platformAuth: {
    douyin: { mode: "none", status: "guest" },
    bilibili: { mode: "none", status: "guest" },
    youtube: { mode: "none", status: "guest" }
  },
  saveDirectory: "C:\\Users\\Video\\StreamVerse",
  downloadMode: "manual",
  qualityPreference: "recommended",
  autoRevealInFinder: false,
  maxConcurrentDownloads: 3,
  proxyUrl: null,
  speedLimit: null,
  autoUpdate: true,
  theme: "dark",
  notifyOnComplete: true,
  language: "zh-CN",
  ffmpegAvailable: false,
  metrics: { todayDownloads: 0, successRate: "--", availableFormats: 0, maxQuality: "--" },
  preview: {
    assetId: "",
    platform: "douyin",
    sourceUrl: "",
    title: "",
    author: "",
    durationSeconds: 0,
    publishDate: "",
    caption: "",
    coverUrl: null,
    coverGradient: "",
    formats: []
  },
  tasks: []
};
