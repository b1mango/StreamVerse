use crate::{DownloadTask, TaskReplayRequest};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const MAX_TASK_HISTORY: usize = 200;

pub type TaskStore = Arc<Mutex<Vec<StoredTaskEntry>>>;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredTaskEntry {
    pub task: DownloadTask,
    pub replay: Option<TaskReplayRequest>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PersistedTaskFile {
    entries: Vec<StoredTaskEntry>,
}

pub fn load_task_store() -> TaskStore {
    Arc::new(Mutex::new(load_entries()))
}

#[cfg(test)]
pub fn new_empty_task_store() -> TaskStore {
    Arc::new(Mutex::new(Vec::new()))
}

pub fn list_tasks(store: &TaskStore) -> Vec<DownloadTask> {
    store
        .lock()
        .unwrap()
        .iter()
        .map(|entry| entry.task.clone())
        .collect()
}

pub fn replay_for_task(store: &TaskStore, task_id: &str) -> Option<TaskReplayRequest> {
    store
        .lock()
        .unwrap()
        .iter()
        .find(|entry| entry.task.id == task_id)
        .and_then(|entry| entry.replay.clone())
}

pub fn set_replay(store: &TaskStore, task_id: &str, replay: TaskReplayRequest) {
    let mut guard = store.lock().unwrap();
    if let Some(entry) = guard.iter_mut().find(|entry| entry.task.id == task_id) {
        entry.replay = Some(replay);
        entry.task.can_retry = true;
        save_entries(&guard);
    }
}

pub fn upsert_task(store: &TaskStore, next: DownloadTask) {
    let mut guard = store.lock().unwrap();
    if let Some(existing) = guard.iter_mut().find(|entry| entry.task.id == next.id) {
        existing.task = next;
    } else {
        guard.insert(
            0,
            StoredTaskEntry {
                task: next,
                replay: None,
            },
        );
    }
    trim_entries(&mut guard);
    save_entries(&guard);
}

pub fn mutate_task<F>(store: &TaskStore, task_id: &str, mutator: F) -> Result<DownloadTask, String>
where
    F: FnOnce(&mut DownloadTask),
{
    let mut guard = store.lock().unwrap();
    let entry = guard
        .iter_mut()
        .find(|entry| entry.task.id == task_id)
        .ok_or_else(|| "未找到对应的下载任务。".to_string())?;
    mutator(&mut entry.task);
    let updated = entry.task.clone();
    save_entries(&guard);
    Ok(updated)
}

pub fn clear_finished(store: &TaskStore) -> Vec<DownloadTask> {
    let mut guard = store.lock().unwrap();
    guard.retain(|entry| {
        !matches!(
            entry.task.status.as_str(),
            "completed" | "failed" | "cancelled"
        )
    });
    save_entries(&guard);
    guard.iter().map(|entry| entry.task.clone()).collect()
}

pub fn normalize_interrupted_tasks(store: &TaskStore) {
    let mut guard = store.lock().unwrap();
    let mut changed = false;

    for entry in guard.iter_mut() {
        if matches!(
            entry.task.status.as_str(),
            "queued" | "downloading" | "paused"
        ) {
            entry.task.status = "failed".to_string();
            entry.task.eta_text = "已中断".to_string();
            entry.task.message =
                Some("上次关闭应用时任务未完成，已标记为中断，可直接重试。".to_string());
            changed = true;
        }
    }

    if changed {
        save_entries(&guard);
    }
}

fn load_entries() -> Vec<StoredTaskEntry> {
    let path = tasks_path();
    let content = fs::read_to_string(path);

    let mut entries = match content {
        Ok(raw) => serde_json::from_str::<PersistedTaskFile>(&raw)
            .map(|file| file.entries)
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };

    trim_entries(&mut entries);
    entries
}

#[cfg(not(test))]
fn save_entries(entries: &[StoredTaskEntry]) {
    let path = tasks_path();

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(content) = serde_json::to_string_pretty(&PersistedTaskFile {
        entries: entries.to_vec(),
    }) {
        let _ = fs::write(path, content);
    }
}

#[cfg(test)]
fn save_entries(_entries: &[StoredTaskEntry]) {}

fn trim_entries(entries: &mut Vec<StoredTaskEntry>) {
    if entries.len() > MAX_TASK_HISTORY {
        entries.truncate(MAX_TASK_HISTORY);
    }
}

fn tasks_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".streamverse").join("tasks.json")
}
