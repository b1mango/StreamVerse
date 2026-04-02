use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

static GLOBAL_HISTORY: OnceLock<DownloadHistoryStore> = OnceLock::new();

const MAX_HISTORY_ENTRIES: usize = 5000;

pub type DownloadHistoryStore = Arc<Mutex<DownloadHistory>>;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadHistoryEntry {
    pub asset_id: String,
    pub platform: String,
    pub title: String,
    pub downloaded_at: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PersistedHistory {
    entries: Vec<DownloadHistoryEntry>,
}

pub struct DownloadHistory {
    entries: Vec<DownloadHistoryEntry>,
    index: HashSet<String>,
}

impl DownloadHistory {
    fn new(entries: Vec<DownloadHistoryEntry>) -> Self {
        let index = entries
            .iter()
            .map(|entry| history_key(&entry.platform, &entry.asset_id))
            .collect();
        Self { entries, index }
    }

    pub fn is_downloaded(&self, platform: &str, asset_id: &str) -> bool {
        self.index.contains(&history_key(platform, asset_id))
    }

    pub fn check_downloaded(&self, platform: &str, asset_ids: &[String]) -> Vec<String> {
        asset_ids
            .iter()
            .filter(|id| self.is_downloaded(platform, id))
            .cloned()
            .collect()
    }

    pub fn record(&mut self, platform: &str, asset_id: &str, title: &str) {
        let key = history_key(platform, asset_id);
        if self.index.contains(&key) {
            return;
        }

        self.index.insert(key);
        self.entries.insert(
            0,
            DownloadHistoryEntry {
                asset_id: asset_id.to_string(),
                platform: platform.to_string(),
                title: title.to_string(),
                downloaded_at: current_timestamp(),
            },
        );

        if self.entries.len() > MAX_HISTORY_ENTRIES {
            if let Some(removed) = self.entries.pop() {
                self.index
                    .remove(&history_key(&removed.platform, &removed.asset_id));
            }
        }

        save_history(&self.entries);
    }

    pub fn total_count(&self) -> usize {
        self.entries.len()
    }
}

pub fn load_history_store() -> DownloadHistoryStore {
    let store = Arc::new(Mutex::new(DownloadHistory::new(load_entries())));
    let _ = GLOBAL_HISTORY.set(Arc::clone(&store));
    store
}

pub fn record_download(platform: &str, asset_id: &str, title: &str) {
    if let Some(store) = GLOBAL_HISTORY.get() {
        if let Ok(mut guard) = store.lock() {
            guard.record(platform, asset_id, title);
        }
    }
}

fn history_key(platform: &str, asset_id: &str) -> String {
    format!("{platform}:{asset_id}")
}

fn current_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{now}")
}

fn load_entries() -> Vec<DownloadHistoryEntry> {
    let path = history_path();
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str::<PersistedHistory>(&raw)
            .map(|file| file.entries)
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_history(entries: &[DownloadHistoryEntry]) {
    let path = history_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(content) = serde_json::to_string_pretty(&PersistedHistory {
        entries: entries.to_vec(),
    }) {
        let _ = fs::write(path, content);
    }
}

fn history_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".streamverse")
        .join("download-history.json")
}
