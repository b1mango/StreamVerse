import type {
  AnalysisProgress,
  AuthStatus,
  ProfileBatch,
  QualityPreference,
  VideoAsset,
  VideoFormat
} from "./types";

// 快照只需要这两个字段；App 侧的 WorkspaceSnapshot 结构兼容
export type YouTubeHydrationSnapshot = {
  profile: ProfileBatch | null;
  selectedProfileFormats: Record<string, string>;
};

export type YouTubeHydrationDeps = {
  analyzeBatchItem: (rawInput: string) => Promise<VideoAsset>;
  pickPreferredFormat: (
    asset: VideoAsset | null,
    preference: QualityPreference,
    authState: AuthStatus
  ) => VideoFormat | undefined;
  currentScope: () => string;
  getProfile: () => ProfileBatch | null;
  setProfile: (batch: ProfileBatch) => void;
  getSnapshot: (scope: string) => YouTubeHydrationSnapshot | undefined;
  setProgress: (scope: string, progress: AnalysisProgress | null) => void;
  getSelectedProfileFormats: () => Record<string, string>;
  setSelectedProfileFormats: (next: Record<string, string>) => void;
  qualityPreference: () => QualityPreference;
  pushToast: (kind: "success" | "error", text: string) => void;
  activeSessionCount: () => number;
};

// YouTube 批量清晰度补全：批次内条目并发解析真实格式，结果提交到
// 当前可见 profile 或所属作用域的快照；hydrationKey 失配时中止孤儿会话
export function createYouTubeBatchHydration(deps: YouTubeHydrationDeps) {
  return async function runYouTubeBatchHydration(
    batch: ProfileBatch,
    ownerScope: string,
    key: string
  ): Promise<number> {
    // 已加载的条目保持原样（恢复或加入会话时避免重复请求）
    const pendingItems = batch.items.map((item) =>
      item.formatStatus === "loaded" ? item : { ...item, formatStatus: "pending" as const }
    );
    // 进度分母是整批条目数，分子从已加载数起算：切回模式重启会话时进度不回退
    const alreadyLoaded = pendingItems.filter((item) => item.formatStatus === "loaded").length;
    let wrapper: ProfileBatch = { ...batch, items: pendingItems };
    // 批次可能属于非当前作用域（并行解析）：不可见时写入该作用域的快照
    const isVisible = () => deps.currentScope() === ownerScope && deps.getProfile()?.hydrationKey === key;
    if (deps.currentScope() === ownerScope) {
      deps.setProfile(wrapper);
    } else {
      const snapshot = deps.getSnapshot(ownerScope);
      if (snapshot) snapshot.profile = wrapper;
    }

    let cursor = 0;
    let completed = 0;
    let failed = 0;
    let orphaned = false;

    // 进度条与文案均显示真实完成数；条目被拾取时同样触发刷新，
    // 不会卡在（0/N）等待首个 yt-dlp 进程返回
    const reportProgress = () => {
      const holder = deps.currentScope() === ownerScope ? deps.getProfile() : deps.getSnapshot(ownerScope)?.profile;
      if (holder?.hydrationKey !== key) return;
      deps.setProgress(ownerScope, {
        current: alreadyLoaded + completed,
        total: pendingItems.length,
        message: `正在读取真实清晰度（已完成 ${alreadyLoaded + completed}/${pendingItems.length}）…`
      });
    };
    reportProgress();

    // 把结果提交到用户能看到的地方：仍停留在该模式则更新实时 profile，
    // 否则写入该作用域的快照，切回时即可看到最新进度；两处都不再有该批次则中止
    const commit = (index: number, nextItem: VideoAsset) => {
      pendingItems[index] = nextItem;
      const nextWrapper: ProfileBatch = { ...wrapper, items: pendingItems };
      const visibleNow = isVisible();
      if (visibleNow) {
        deps.setProfile(nextWrapper);
      } else {
        const snapshot = deps.getSnapshot(ownerScope);
        if (snapshot?.profile?.hydrationKey === key) {
          snapshot.profile = nextWrapper;
        } else {
          orphaned = true;
        }
      }
      wrapper = nextWrapper;
      const selected = deps.pickPreferredFormat(nextItem, deps.qualityPreference(), "active")?.id;
      if (selected) {
        if (visibleNow) {
          if (!deps.getSelectedProfileFormats()[nextItem.assetId]) {
            deps.setSelectedProfileFormats({ ...deps.getSelectedProfileFormats(), [nextItem.assetId]: selected });
          }
        } else {
          const snapshot = deps.getSnapshot(ownerScope);
          if (snapshot && !snapshot.selectedProfileFormats[nextItem.assetId]) {
            snapshot.selectedProfileFormats = { ...snapshot.selectedProfileFormats, [nextItem.assetId]: selected };
          }
        }
      }
      reportProgress();
    };

    const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

    // 单条目带退避重试：限流/人机验证类错误用更长的退避，普通瞬时错误快速重试
    const throttleHint = /not a bot|sign in|429|too many requests|503|timed out|timeout|unavailable/i;
    const resolveItem = async (item: VideoAsset): Promise<VideoAsset> => {
      let throttled = false;
      for (let attempt = 0; attempt < 4; attempt += 1) {
        if (orphaned) break;
        if (attempt > 0) {
          await sleep(
            throttled ? 4000 * attempt + Math.random() * 2500 : 900 * attempt + Math.random() * 600
          );
        }
        try {
          const resolved = await deps.analyzeBatchItem(item.sourceUrl);
          if (resolved.formats.length > 0) {
            return {
              ...item,
              ...resolved,
              categoryLabel: item.categoryLabel,
              groupTitle: item.groupTitle,
              formatStatus: "loaded" as const
            };
          }
        } catch (error) {
          console.warn("[youtube-hydration] 条目解析失败", item.sourceUrl, error);
          if (error instanceof Error && throttleHint.test(error.message)) throttled = true;
        }
      }
      return { ...item, formatStatus: "failed" as const };
    };

    const worker = async () => {
      while (!orphaned) {
        const index = cursor;
        cursor += 1;
        const item = pendingItems[index];
        if (!item) return;
        // 切回模式重启会话时跳过已加载条目，不重复解析
        if (item.formatStatus === "loaded") continue;

        const nextItem = await resolveItem(item);
        if (orphaned) return;
        completed += 1;
        if (nextItem.formatStatus === "failed") failed += 1;
        commit(index, nextItem);
      }
    };

    // 两个批次并行补全时各自减半并发，避免同时打满 YouTube 限流阈值
    const workers = Math.min(deps.activeSessionCount() > 1 ? 3 : 6, pendingItems.length);
    // 错开启动工人，削掉瞬时并发峰值，降低触发 YouTube 限流的概率
    await Promise.all(Array.from({ length: workers }, (_, i) => sleep(i * 260).then(worker)));
    const holder = deps.currentScope() === ownerScope ? deps.getProfile() : deps.getSnapshot(ownerScope)?.profile;
    if (holder?.hydrationKey === key) {
      deps.setProgress(ownerScope, null);
      if (failed > 0) {
        deps.pushToast("error", `${batch.items.length - failed} 个视频已读取真实清晰度，${failed} 个读取失败（可能是 YouTube 限流，稍后重新解析会只补读失败项）。`);
      }
    }
    return failed;
  };
}
