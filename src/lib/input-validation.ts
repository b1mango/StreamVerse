import type { PlatformId } from "./types";

export type WorkflowMode = "single" | "profile" | "playlist";

export interface InputTarget {
  platform: PlatformId;
  mode: WorkflowMode | "unknown";
}

const PLATFORM_LABELS: Record<PlatformId, string> = {
  douyin: "抖音",
  bilibili: "Bilibili",
  youtube: "YouTube"
};

function modeLabel(platform: PlatformId, mode: Exclude<InputTarget["mode"], "unknown">) {
  if (mode === "single") return "单视频";
  if (mode === "playlist") return "合集";
  return platform === "youtube" ? "主页频道" : "主页批量";
}

function normalizedInput(value: string) {
  return value.trim().toLowerCase();
}

export function detectInputTarget(value: string): InputTarget | null {
  const input = normalizedInput(value);
  if (!input) return null;

  if (/\b(bv[0-9a-z]{10,})\b/i.test(value) || input.includes("bilibili.com") || input.includes("b23.tv")) {
    const mode = input.includes("space.bilibili.com")
      ? "profile"
      : input.includes("b23.tv") || /\/(?:video|bangumi\/play)(?:\/|\?|$)/.test(input) || /\bbv[0-9a-z]{10,}\b/i.test(value)
        ? "single"
        : "unknown";
    return {
      platform: "bilibili",
      mode
    };
  }

  if (input.includes("douyin.com") || input.includes("iesdouyin.com")) {
    // 抖音单视频只接受分享文案中的短链；网页地址仅支持创作者主页（/user/）
    const mode = /\/user(?:\/|\?|$)/.test(input) ? "profile" : "unknown";
    return { platform: "douyin", mode };
  }

  if (input.includes("youtube.com") || input.includes("youtu.be")) {
    const mode = input.includes("youtu.be/") || /\/(?:watch|shorts|live)(?:\/|\?|$)/.test(input)
      ? "single"
      : /youtube\.com\/playlist\?[^#]*\blist=/.test(input)
        ? "playlist"
        : /youtube\.com\/(?:@[^/?#]+|channel\/[^/?#]+|c\/[^/?#]+|user\/[^/?#]+)(?:\/(?:featured|videos|shorts|streams))?(?:[/?#]|$)/.test(input)
        ? "profile"
        : "unknown";
    return { platform: "youtube", mode };
  }

  return null;
}

export function validateInputTarget(
  value: string,
  selectedPlatform: PlatformId,
  selectedMode: WorkflowMode
): string | null {
  const target = detectInputTarget(value);
  if (!target) {
    return "未识别到受支持的作品或主页链接，请检查链接是否完整。";
  }

  if (target.platform !== selectedPlatform) {
    const detectedMode = target.mode === "unknown" ? "" : modeLabel(target.platform, target.mode);
    return `检测到这是${PLATFORM_LABELS[target.platform]}${detectedMode}链接，请切换到对应模块后再解析。`;
  }

  if (target.mode === "unknown") {
    const isDouyinShortLink =
      target.platform === "douyin" && (/v\.douyin\.com\//i.test(value) || /iesdouyin\.com\/share\//i.test(value));
    if (!isDouyinShortLink) {
      if (target.platform === "douyin") {
        return "抖音单视频请粘贴完整分享文案（含 v.douyin.com 短链）；解析主页可用 https://www.douyin.com/user/… 地址或主页分享文案。";
      }
      return `这是${PLATFORM_LABELS[target.platform]}链接，但不是当前支持的作品或主页链接，请复制具体作品页或创作者主页地址。`;
    }
  }

  if (target.mode !== "unknown" && target.mode !== selectedMode) {
    const detectedMode = modeLabel(target.platform, target.mode);
    return `检测到这是${PLATFORM_LABELS[target.platform]}${detectedMode}链接，请切换到${PLATFORM_LABELS[target.platform]}${detectedMode}模块。`;
  }

  return null;
}
