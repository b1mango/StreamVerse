import type {
  ProfileBatch,
  ProfileItemPatch,
  ProfileSessionEvent
} from "./types";

interface ProfileSessionState {
  profileBatch: ProfileBatch | null;
  statusByAssetId: Record<string, ProfileItemPatch>;
  completed: boolean;
  errorMessage: string;
}

function emptyState(): ProfileSessionState {
  return {
    profileBatch: null,
    statusByAssetId: {},
    completed: false,
    errorMessage: ""
  };
}

export function createProfileSessionState() {
  const state = $state<ProfileSessionState>(emptyState());

  function reset() {
    const next = emptyState();
    state.profileBatch = next.profileBatch;
    state.statusByAssetId = next.statusByAssetId;
    state.completed = next.completed;
    state.errorMessage = next.errorMessage;
  }

  function applyEvent(event: ProfileSessionEvent) {
    switch (event.event) {
      case "started":
        state.profileBatch = {
          profileTitle: event.data.profileTitle,
          sourceUrl: event.data.sourceUrl,
          totalAvailable: event.data.totalAvailable,
          fetchedCount: 0,
          skippedCount: 0,
          sessionCookieFile: null,
          items: []
        };
        state.completed = false;
        state.errorMessage = "";
        break;

      case "itemsAppended": {
        const batch = state.profileBatch ?? {
          profileTitle: "",
          sourceUrl: "",
          totalAvailable: event.data.items.length,
          fetchedCount: 0,
          skippedCount: 0,
          sessionCookieFile: null,
          items: []
        };
        const existingIds = new Set(batch.items.map((item) => item.assetId));
        const nextItems = [
          ...batch.items,
          ...event.data.items.filter((item) => !existingIds.has(item.assetId))
        ];
        state.profileBatch = {
          ...batch,
          fetchedCount: nextItems.length,
          items: nextItems
        };
        break;
      }

      case "itemPatched":
        state.statusByAssetId = {
          ...state.statusByAssetId,
          [event.data.patch.assetId]: event.data.patch
        };
        break;

      case "completed":
        if (state.profileBatch) {
          state.profileBatch = {
            ...state.profileBatch,
            fetchedCount: event.data.fetchedCount,
            skippedCount: event.data.skippedCount,
            sessionCookieFile: event.data.sessionCookieFile ?? null
          };
        }
        state.completed = true;
        break;

      case "failed":
        state.errorMessage = event.data.message;
        state.completed = false;
        break;
    }
  }

  return {
    state,
    reset,
    applyEvent
  };
}
