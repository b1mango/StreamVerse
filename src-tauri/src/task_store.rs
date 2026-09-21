use crate::{DownloadRequest, DownloadTask};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;

const MAX_TASK_HISTORY: usize = 200;
const PROGRESS_THROTTLE_MS: u64 = 300;

pub type TaskStore = Arc<TaskStoreInner>;

pub struct TaskStoreInner {
    pub entries: Mutex<Vec<StoredTaskEntry>>,
    dirty: AtomicBool,
    app_handle: Mutex<Option<tauri::AppHandle>>,
    last_emit: Mutex<HashMap<String, Instant>>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredTaskEntry {
    pub task: DownloadTask,
    pub replay: Option<DownloadRequest>,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum TaskEvent {
    Upsert { task: DownloadTask },
    Delete { task_id: String },
    Reset { tasks: Vec<DownloadTask> },
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PersistedTaskFile {
    entries: Vec<StoredTaskEntry>,
}

pub fn load_task_store() -> TaskStore {
    let store = Arc::new(TaskStoreInner {
        entries: Mutex::new(load_entries()),
        dirty: AtomicBool::new(false),
        app_handle: Mutex::new(None),
        last_emit: Mutex::new(HashMap::new()),
    });

    let flusher = Arc::clone(&store);
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(2));
        if flusher.dirty.swap(false, Ordering::AcqRel) {
            let guard = flusher.entries.lock().unwrap();
            if save_entries(&guard).is_err() {
                flusher.dirty.store(true, Ordering::Release);
            }
        }
    });

    store
}

#[cfg(test)]
pub fn new_empty_task_store() -> TaskStore {
    Arc::new(TaskStoreInner {
        entries: Mutex::new(Vec::new()),
        dirty: AtomicBool::new(false),
        app_handle: Mutex::new(None),
        last_emit: Mutex::new(HashMap::new()),
    })
}

/// Inject the Tauri AppHandle so events can be emitted.
pub fn set_app_handle(store: &TaskStore, handle: tauri::AppHandle) {
    *store.app_handle.lock().unwrap() = Some(handle);
}

pub fn list_tasks(store: &TaskStore) -> Vec<DownloadTask> {
    store
        .entries
        .lock()
        .unwrap()
        .iter()
        .map(|entry| entry.task.clone())
        .collect()
}

pub fn replay_for_task(store: &TaskStore, task_id: &str) -> Option<DownloadRequest> {
    store
        .entries
        .lock()
        .unwrap()
        .iter()
        .find(|entry| entry.task.id == task_id)
        .and_then(|entry| entry.replay.clone())
}

pub fn set_replay(store: &TaskStore, task_id: &str, replay: DownloadRequest) {
    let mut guard = store.entries.lock().unwrap();
    if let Some(entry) = guard.iter_mut().find(|entry| entry.task.id == task_id) {
        entry.replay = Some(replay);
        entry.task.can_retry = true;
        store.dirty.store(true, Ordering::Release);
        if save_entries(&guard).is_ok() {
            store.dirty.store(false, Ordering::Release);
        }
    }
}

pub fn upsert_task(store: &TaskStore, next: DownloadTask) {
    let emitted = next.clone();
    let mut guard = store.entries.lock().unwrap();
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
    store.dirty.store(true, Ordering::Release);
    emit_task_event(store, TaskEvent::Upsert { task: emitted });
}

pub fn mutate_task<F>(store: &TaskStore, task_id: &str, mutator: F) -> Result<DownloadTask, String>
where
    F: FnOnce(&mut DownloadTask),
{
    let mut guard = store.entries.lock().unwrap();
    let entry = guard
        .iter_mut()
        .find(|entry| entry.task.id == task_id)
        .ok_or_else(|| "未找到对应的下载任务。".to_string())?;
    mutator(&mut entry.task);
    let updated = entry.task.clone();
    store.dirty.store(true, Ordering::Release);
    if save_entries(&guard).is_ok() {
        store.dirty.store(false, Ordering::Release);
    }
    emit_task_event(
        store,
        TaskEvent::Upsert {
            task: updated.clone(),
        },
    );
    Ok(updated)
}

/// Force any pending dirty writes to disk immediately.
#[allow(dead_code)]
pub fn flush(store: &TaskStore) {
    if store.dirty.swap(false, Ordering::AcqRel) {
        let guard = store.entries.lock().unwrap();
        store.dirty.store(true, Ordering::Release);
        if save_entries(&guard).is_ok() {
            store.dirty.store(false, Ordering::Release);
        }
    }
}

pub fn remove_task(store: &TaskStore, task_id: &str) -> Result<(), String> {
    let mut guard = store.entries.lock().unwrap();
    let len_before = guard.len();
    guard.retain(|entry| entry.task.id != task_id);
    if guard.len() == len_before {
        return Err("未找到对应的下载任务。".to_string());
    }
    store.dirty.store(true, Ordering::Release);
    if save_entries(&guard).is_ok() {
        store.dirty.store(false, Ordering::Release);
    }
    emit_task_event(
        store,
        TaskEvent::Delete {
            task_id: task_id.to_string(),
        },
    );
    Ok(())
}

pub fn clear_finished(store: &TaskStore) -> Vec<DownloadTask> {
    let mut guard = store.entries.lock().unwrap();
    guard.retain(|entry| {
        !matches!(
            entry.task.status.as_str(),
            "completed" | "failed" | "cancelled"
        )
    });
    store.dirty.store(true, Ordering::Release);
    if save_entries(&guard).is_ok() {
        store.dirty.store(false, Ordering::Release);
    }
    let result: Vec<DownloadTask> = guard.iter().map(|entry| entry.task.clone()).collect();
    emit_task_event(
        store,
        TaskEvent::Reset {
            tasks: result.clone(),
        },
    );
    result
}

pub fn normalize_interrupted_tasks(store: &TaskStore) {
    let mut guard = store.entries.lock().unwrap();
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
        store.dirty.store(true, Ordering::Release);
        if save_entries(&guard).is_ok() {
            store.dirty.store(false, Ordering::Release);
        }
    }
}

fn load_entries() -> Vec<StoredTaskEntry> {
    let path = tasks_path();
    let mut entries = crate::persistence::load_json::<PersistedTaskFile>(&path).entries;

    trim_entries(&mut entries);
    entries
}

#[cfg(not(test))]
fn save_entries(entries: &[StoredTaskEntry]) -> Result<(), String> {
    crate::persistence::save_json(
        &tasks_path(),
        &PersistedTaskFile {
            entries: entries.to_vec(),
        },
    )
}

#[cfg(test)]
fn save_entries(_entries: &[StoredTaskEntry]) -> Result<(), String> {
    Ok(())
}

fn trim_entries(entries: &mut Vec<StoredTaskEntry>) {
    if entries.len() > MAX_TASK_HISTORY {
        entries.truncate(MAX_TASK_HISTORY);
    }
}

fn tasks_path() -> PathBuf {
    crate::settings::app_data_root().join("tasks-v2.json")
}

fn emit_task_event(store: &TaskStore, event: TaskEvent) {
    let now = Instant::now();
    let (key, force) = match &event {
        TaskEvent::Upsert { task } => (
            task.id.clone(),
            matches!(
                task.status.as_str(),
                "completed" | "failed" | "cancelled" | "paused"
            ),
        ),
        TaskEvent::Delete { task_id } => (task_id.clone(), true),
        TaskEvent::Reset { .. } => ("__reset__".to_string(), true),
    };
    let mut last_map = store.last_emit.lock().unwrap();
    let should_emit = force
        || last_map.get(&key).is_none_or(|last| {
            now.duration_since(*last).as_millis() as u64 >= PROGRESS_THROTTLE_MS
        });

    if should_emit {
        last_map.insert(key, now);
        drop(last_map);
        if let Some(handle) = store.app_handle.lock().unwrap().as_ref() {
            let _ = handle.emit("task-event", event);
        }
    }
}
