import { writable, derived, get } from "svelte/store";
import type { LanguageCode } from "./types";

const translations: Record<LanguageCode, Record<string, string>> = {
  "zh-CN": {
    // Header
    "app.title": "StreamVerse",
    "app.settings": "设置",
    "app.taskQueue": "下载队列",
    "app.loading": "正在建立下载工作台…",
    "app.backToHome": "返回平台首页",
    "app.settingsSaved": "设置已保存。",

    // Settings
    "settings.title": "应用设置",
    "settings.close": "关闭",
    "settings.save": "保存设置",
    "settings.saving": "保存中…",
    "settings.downloadPath": "默认下载路径",
    "settings.downloadPathPlaceholder": "选择或输入默认下载目录",
    "settings.pickDirectory": "选择目录",
    "settings.pickingDirectory": "选择中…",
    "settings.loginSource": "登录源",
    "settings.qualityStrategy": "默认清晰度策略",
    "settings.autoReveal": "下载完成后自动在文件管理器中定位文件",
    "settings.maxConcurrent": "最大并行下载数",
    "settings.proxy": "代理地址",
    "settings.proxyPlaceholder": "例如 http://127.0.0.1:7890",
    "settings.speedLimit": "下载限速",
    "settings.speedLimitPlaceholder": "例如 5M（留空不限速）",
    "settings.theme": "主题",
    "settings.language": "语言",
    "settings.notifyOnComplete": "下载完成后发送系统通知",
    "settings.autoUpdate": "自动检查更新",
    "settings.ffmpegDetected": "已检测到",
    "settings.ffmpegMissing": "未检测到",

    // Theme
    "theme.dark": "深色模式",
    "theme.light": "浅色模式",

    // Auth
    "auth.guest": "游客模式",
    "auth.active": "已登录",
    "auth.expired": "登录失效",

    // Quality strategies
    "quality.recommended": "推荐优先",
    "quality.highest": "最高质量",
    "quality.no_watermark": "无水印优先",
    "quality.smallest": "最小体积",

    // Notifications
    "notify.downloadComplete": "下载完成",
    "notify.taskCompleted": "任务已完成",
    "notify.tasksCompleted": "个任务已完成",

    // Common
    "common.notLoggedIn": "未登录",
    "common.notSelected": "未选择",
    "common.analyze": "解析",
    "common.download": "下载",
    "common.cancel": "取消",
    "common.retry": "重试",
    "common.pause": "暂停",
    "common.resume": "恢复",
    "common.close": "关闭",
    "common.selectAll": "全选",
    "common.deselectAll": "取消全选",
    "common.alreadyDownloaded": "已下载",
    "common.analyzing": "解析中…",
    "common.loading": "读取中…",
    "common.unit": "个",

    // Content types
    "content.video": "视频",
    "content.cover": "封面",
    "content.caption": "文案",
    "content.metadata": "元数据",

    // Format tags
    "format.recommended": "推荐",
    "format.noWatermark": "无水印",
    "format.requiresLogin": "登录后",

    // Home
    "home.title": "选择功能",
    "home.installed": "已内置",
    "home.notInstalled": "当前构建未包含",
    "home.version": "版本",
    "home.open": "打开",

    // Module labels
    "module.douyin-single.label": "抖音单视频下载",
    "module.douyin-single.description": "分享文案、短链、作品页链接解析与下载。",
    "module.douyin-profile.label": "抖音主页批量下载",
    "module.douyin-profile.description": "打开浏览器登录后读取主页作品，再勾选入队。",
    "module.bilibili-single.label": "Bilibili 单视频下载",
    "module.bilibili-single.description": "支持单视频格式解析与高质量合流下载。",
    "module.bilibili-profile.label": "Bilibili 主页批量下载",
    "module.bilibili-profile.description": "读取 UP 主投稿列表后批量入队。",
    "module.youtube-single.label": "YouTube 单视频下载",
    "module.youtube-single.description": "解析 YouTube 视频并按所选格式下载。",

    // Single video
    "single.downloadContent": "下载内容",
    "single.analyze": "解析作品",
    "single.pasteAndAnalyze": "粘贴并解析",
    "single.creatingTask": "创建任务…",
    "single.startDownload": "开始下载",
    "single.analyzeProgress": "解析进度",
    "single.platform": "平台",
    "single.defaultStrategy": "默认策略",
    "single.currentFormat": "当前格式",
    "single.awaitingAnalysis": "等待解析",
    "single.selectQuality": "选择清晰度",
    "single.formatsCount": "个格式",
    "single.noVideoSelected": "当前只保存附加内容，所以这里不需要选择清晰度。",
    "single.author": "作者",
    "single.duration": "时长",
    "single.publishDate": "发布日期",
    "single.taskCreated": "已创建下载任务。",
    "single.taskStarted": "下载任务已开始。",

    // Profile batch
    "batch.downloadContent": "批量下载内容",
    "batch.selectAtLeastOne": "请至少勾选一个主页作品。",
    "batch.selectAtLeastOneOption": "至少要选择一种要保存的内容。",
    "batch.noMatch": "没有匹配的",
    "batch.enqueue": "开始批量下载",
    "batch.invertSelection": "反选",
    "batch.clearSelection": "清空",
    "batch.fetched": "已读取",
    "batch.selected": "已选",
    "batch.filterPlaceholder": "筛选标题",
    "batch.formatLabel": "下载清晰度",
    "batch.awaitingSelection": "等待选择",
    "batch.initializing": "正在初始化，请稍候…",

    // Task panel
    "task.recentTasks": "最近任务",
    "task.clearing": "清理中…",
    "task.clearFinished": "清理已完成",
    "task.empty": "还没有下载任务。先进入上面的平台工作区创建一个试试。",
    "task.revealFile": "定位文件",
    "task.idle": "空闲",
    "task.analyzing": "解析中",
    "task.queued": "排队中",
    "task.downloading": "下载中",
    "task.paused": "已暂停",
    "task.cancelled": "已取消",
    "task.completed": "已完成",
    "task.failed": "失败",

    // Douyin specific props
    "douyin.heading": "抖音单视频下载",
    "douyin.placeholder": "粘贴抖音分享文案、短链或作品页链接",
    "douyin.profileHeading": "抖音主页批量下载",
    "douyin.profilePlaceholder": "粘贴抖音个人主页分享文案或主页链接",
    "douyin.itemLabel": "作品",
    "douyin.pasteLabel": "粘贴链接",
    "douyin.pasteLoading": "读取剪贴板…",
    "douyin.openBrowser": "打开浏览器",
    "douyin.openingBrowser": "打开中…",
    "douyin.analyzeLabel": "读取完整列表",
    "douyin.analyzeLoading": "读取中…",

    // Bilibili specific props
    "bilibili.heading": "Bilibili 单视频下载",
    "bilibili.placeholder": "粘贴 Bilibili 视频链接、分享文案、BV 号或 b23.tv 短链",
    "bilibili.profileHeading": "Bilibili 主页批量下载",
    "bilibili.profilePlaceholder": "粘贴 Bilibili UP 主空间页链接或分享文案",
    "bilibili.itemLabel": "视频",
    "bilibili.analyzeLabel": "解析主页",
    "bilibili.analyzeLoading": "读取视频中…",
    "bilibili.enqueueLabel": "将所选视频加入队列",
    "bilibili.enqueuingLabel": "加入队列中…",

    // YouTube specific props
    "youtube.heading": "YouTube 单视频下载",
    "youtube.placeholder": "粘贴 YouTube 视频链接、分享文案或 youtu.be 短链",
  },
  en: {
    // Header
    "app.title": "StreamVerse",
    "app.settings": "Settings",
    "app.taskQueue": "Download Queue",
    "app.loading": "Setting up workspace…",
    "app.backToHome": "Back to Home",
    "app.settingsSaved": "Settings saved.",

    // Settings
    "settings.title": "App Settings",
    "settings.close": "Close",
    "settings.save": "Save Settings",
    "settings.saving": "Saving…",
    "settings.downloadPath": "Default Download Path",
    "settings.downloadPathPlaceholder": "Select or enter download directory",
    "settings.pickDirectory": "Browse",
    "settings.pickingDirectory": "Browsing…",
    "settings.loginSource": "Login Source",
    "settings.qualityStrategy": "Default Quality Strategy",
    "settings.autoReveal": "Reveal file in file manager after download",
    "settings.maxConcurrent": "Max Concurrent Downloads",
    "settings.proxy": "Proxy URL",
    "settings.proxyPlaceholder": "e.g. http://127.0.0.1:7890",
    "settings.speedLimit": "Speed Limit",
    "settings.speedLimitPlaceholder": "e.g. 5M (empty = unlimited)",
    "settings.theme": "Theme",
    "settings.language": "Language",
    "settings.notifyOnComplete": "Send system notification on download complete",
    "settings.autoUpdate": "Auto-check for updates",
    "settings.ffmpegDetected": "Detected",
    "settings.ffmpegMissing": "Not detected",

    // Theme
    "theme.dark": "Dark Mode",
    "theme.light": "Light Mode",

    // Auth
    "auth.guest": "Guest",
    "auth.active": "Logged in",
    "auth.expired": "Session expired",

    // Quality strategies
    "quality.recommended": "Recommended",
    "quality.highest": "Highest Quality",
    "quality.no_watermark": "No Watermark",
    "quality.smallest": "Smallest Size",

    // Notifications
    "notify.downloadComplete": "Download Complete",
    "notify.taskCompleted": "Task completed",
    "notify.tasksCompleted": "tasks completed",

    // Common
    "common.notLoggedIn": "Not logged in",
    "common.notSelected": "Not selected",
    "common.analyze": "Analyze",
    "common.download": "Download",
    "common.cancel": "Cancel",
    "common.retry": "Retry",
    "common.pause": "Pause",
    "common.resume": "Resume",
    "common.close": "Close",
    "common.selectAll": "Select All",
    "common.deselectAll": "Deselect All",
    "common.alreadyDownloaded": "Downloaded",
    "common.analyzing": "Analyzing…",
    "common.loading": "Loading…",
    "common.unit": "",

    // Content types
    "content.video": "Video",
    "content.cover": "Cover",
    "content.caption": "Caption",
    "content.metadata": "Metadata",

    // Format tags
    "format.recommended": "Best",
    "format.noWatermark": "No Watermark",
    "format.requiresLogin": "Login Required",

    // Home
    "home.title": "Choose a Module",
    "home.installed": "Built-in",
    "home.notInstalled": "Not included in this build",
    "home.version": "v",
    "home.open": "Open",

    // Module labels
    "module.douyin-single.label": "Douyin Single Video",
    "module.douyin-single.description": "Download from share link, short URL, or video page.",
    "module.douyin-profile.label": "Douyin Profile Batch",
    "module.douyin-profile.description": "Open browser to login, then read and queue profile videos.",
    "module.bilibili-single.label": "Bilibili Single Video",
    "module.bilibili-single.description": "Parse and download single video with high-quality muxing.",
    "module.bilibili-profile.label": "Bilibili Creator Batch",
    "module.bilibili-profile.description": "Read creator uploads and batch enqueue.",
    "module.youtube-single.label": "YouTube Single Video",
    "module.youtube-single.description": "Parse YouTube video and download in selected format.",

    // Single video
    "single.downloadContent": "Download Content",
    "single.analyze": "Analyze",
    "single.pasteAndAnalyze": "Paste & Analyze",
    "single.creatingTask": "Creating…",
    "single.startDownload": "Start Download",
    "single.analyzeProgress": "Analysis Progress",
    "single.platform": "Platform",
    "single.defaultStrategy": "Strategy",
    "single.currentFormat": "Format",
    "single.awaitingAnalysis": "Awaiting",
    "single.selectQuality": "Select Quality",
    "single.formatsCount": "formats",
    "single.noVideoSelected": "Only saving extras — no quality selection needed.",
    "single.author": "Author",
    "single.duration": "Duration",
    "single.publishDate": "Published",
    "single.taskCreated": "Download task created.",
    "single.taskStarted": "Download started.",

    // Profile batch
    "batch.downloadContent": "Batch Download Content",
    "batch.selectAtLeastOne": "Please select at least one item.",
    "batch.selectAtLeastOneOption": "Please select at least one download option.",
    "batch.noMatch": "No matching ",
    "batch.enqueue": "Start Batch Download",
    "batch.invertSelection": "Invert",
    "batch.clearSelection": "Clear",
    "batch.fetched": "Fetched",
    "batch.selected": "Selected",
    "batch.filterPlaceholder": "Filter by title",
    "batch.formatLabel": "Quality",
    "batch.awaitingSelection": "Select…",
    "batch.initializing": "Initializing, please wait…",

    // Task panel
    "task.recentTasks": "Recent Tasks",
    "task.clearing": "Clearing…",
    "task.clearFinished": "Clear Finished",
    "task.empty": "No download tasks yet. Choose a platform above to get started.",
    "task.revealFile": "Reveal",
    "task.idle": "Idle",
    "task.analyzing": "Analyzing",
    "task.queued": "Queued",
    "task.downloading": "Downloading",
    "task.paused": "Paused",
    "task.cancelled": "Cancelled",
    "task.completed": "Completed",
    "task.failed": "Failed",

    // Douyin specific props
    "douyin.heading": "Douyin Single Video",
    "douyin.placeholder": "Paste Douyin share text, short link, or video page URL",
    "douyin.profileHeading": "Douyin Profile Batch",
    "douyin.profilePlaceholder": "Paste Douyin profile share text or profile URL",
    "douyin.itemLabel": "video",
    "douyin.pasteLabel": "Paste Link",
    "douyin.pasteLoading": "Reading clipboard…",
    "douyin.openBrowser": "Open Browser",
    "douyin.openingBrowser": "Opening…",
    "douyin.analyzeLabel": "Read Full List",
    "douyin.analyzeLoading": "Reading…",

    // Bilibili specific props
    "bilibili.heading": "Bilibili Single Video",
    "bilibili.placeholder": "Paste Bilibili video link, share text, BV number, or b23.tv short link",
    "bilibili.profileHeading": "Bilibili Creator Batch",
    "bilibili.profilePlaceholder": "Paste Bilibili creator space URL or share text",
    "bilibili.itemLabel": "video",
    "bilibili.analyzeLabel": "Parse Profile",
    "bilibili.analyzeLoading": "Reading videos…",
    "bilibili.enqueueLabel": "Enqueue selected videos",
    "bilibili.enqueuingLabel": "Enqueuing…",

    // YouTube specific props
    "youtube.heading": "YouTube Single Video",
    "youtube.placeholder": "Paste YouTube video link, share text, or youtu.be short link",
  }
};

let currentLang: LanguageCode = "zh-CN";
const langStore = writable<LanguageCode>("zh-CN");

export function setLanguage(lang: LanguageCode) {
  currentLang = lang;
  langStore.set(lang);
}

/** Svelte-store version: use as `$t('key')` in templates for reactive i18n. */
export const t = derived(langStore, ($lang) => {
  return (key: string): string => {
    return translations[$lang]?.[key] ?? translations["zh-CN"]?.[key] ?? key;
  };
});

/** Imperative version for use outside Svelte templates (e.g. in callbacks). */
export function tRaw(key: string): string {
  return translations[currentLang]?.[key] ?? translations["zh-CN"]?.[key] ?? key;
}

export function getLanguage(): LanguageCode {
  return currentLang;
}
