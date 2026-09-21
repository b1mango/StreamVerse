use crate::{task_store, DownloadTask};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

static DOWNLOAD_SEMAPHORE: OnceLock<Arc<(Mutex<u32>, Condvar)>> = OnceLock::new();
static MAX_CONCURRENT: OnceLock<AtomicU64> = OnceLock::new();
static NETWORK_PROXY: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static NETWORK_SPEED_LIMIT: OnceLock<Mutex<Option<String>>> = OnceLock::new();

type DownloadJob = Box<dyn FnOnce() + Send + 'static>;

#[derive(Default)]
struct DownloadPool {
    queue: VecDeque<DownloadJob>,
    workers: usize,
}

static DOWNLOAD_POOL: OnceLock<Arc<(Mutex<DownloadPool>, Condvar)>> = OnceLock::new();

fn download_pool() -> Arc<(Mutex<DownloadPool>, Condvar)> {
    Arc::clone(
        DOWNLOAD_POOL
            .get_or_init(|| Arc::new((Mutex::new(DownloadPool::default()), Condvar::new()))),
    )
}

fn max_concurrent_downloads() -> usize {
    MAX_CONCURRENT
        .get()
        .map(|value| value.load(Ordering::Relaxed) as usize)
        .unwrap_or(3)
}

/// 把下载任务交给有界 worker 池执行：同时存活的下载线程不超过
/// maxConcurrentDownloads，排队任务只占用队列节点而不再各自占用一个线程。
/// 真正的并发闸门仍是 acquire_download_slot 的信号量（支持运行时调整上限）。
pub(super) fn spawn_download_job(job: impl FnOnce() + Send + 'static) {
    let pool = download_pool();
    let spawn_worker = {
        let (lock, _) = pool.as_ref();
        let mut state = lock.lock().unwrap();
        state.queue.push_back(Box::new(job));
        if state.workers < max_concurrent_downloads() {
            state.workers += 1;
            true
        } else {
            false
        }
    };
    if spawn_worker {
        let worker_pool = Arc::clone(&pool);
        thread::spawn(move || download_pool_worker(worker_pool));
    }
    let (_, condition) = pool.as_ref();
    condition.notify_one();
}

fn download_pool_worker(pool: Arc<(Mutex<DownloadPool>, Condvar)>) {
    loop {
        let job = {
            let (lock, condition) = pool.as_ref();
            let mut state = lock.lock().unwrap();
            loop {
                if let Some(job) = state.queue.pop_front() {
                    break job;
                }
                state = condition.wait(state).unwrap();
            }
        };
        // 单个任务 panic 不应拖垮整个 worker 池
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
    }
}

#[derive(Clone)]
pub struct TaskController {
    supports_pause: bool,
    supports_cancel: bool,
    pause_requested: Arc<AtomicBool>,
    cancel_requested: Arc<AtomicBool>,
}

pub type TaskControllerStore = Arc<Mutex<HashMap<String, Arc<TaskController>>>>;

impl TaskController {
    pub(super) fn new(supports_pause: bool, supports_cancel: bool) -> Self {
        Self {
            supports_pause,
            supports_cancel,
            pause_requested: Arc::new(AtomicBool::new(false)),
            cancel_requested: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(super) fn is_pause_requested(&self) -> bool {
        self.pause_requested.load(Ordering::Relaxed)
    }

    pub(super) fn is_cancel_requested(&self) -> bool {
        self.cancel_requested.load(Ordering::Relaxed)
    }

    fn request_pause(&self) {
        if self.supports_pause {
            self.pause_requested.store(true, Ordering::Relaxed);
        }
    }

    fn request_resume(&self) {
        self.pause_requested.store(false, Ordering::Relaxed);
    }

    pub(super) fn request_cancel(&self) {
        if self.supports_cancel {
            self.cancel_requested.store(true, Ordering::Relaxed);
        }
    }
}

pub fn new_task_controller_store() -> TaskControllerStore {
    Arc::new(Mutex::new(HashMap::new()))
}

pub fn set_max_concurrent_downloads(max: u32) {
    let max = max.clamp(1, 10) as u64;
    MAX_CONCURRENT
        .get_or_init(|| AtomicU64::new(max))
        .store(max, Ordering::Relaxed);
    DOWNLOAD_SEMAPHORE.get_or_init(|| Arc::new((Mutex::new(0), Condvar::new())));
}

pub fn set_network_settings(proxy: Option<String>, speed_limit: Option<String>) {
    *NETWORK_PROXY
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap() = proxy;
    *NETWORK_SPEED_LIMIT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap() = speed_limit;
}

pub(super) fn current_proxy_for_platform(platform: &str) -> Option<String> {
    if platform == "youtube" {
        NETWORK_PROXY
            .get()
            .and_then(|value| value.lock().ok())
            .and_then(|value| value.clone())
    } else {
        None
    }
}

pub(super) fn current_speed_limit() -> Option<String> {
    NETWORK_SPEED_LIMIT
        .get()
        .and_then(|value| value.lock().ok())
        .and_then(|value| value.clone())
}

pub(super) fn parse_speed_limit_bytes(limit: Option<&str>) -> Option<u64> {
    let limit = limit?.trim();
    if limit.is_empty() {
        return None;
    }
    let (number, unit) = if limit.ends_with(['M', 'm']) {
        (&limit[..limit.len() - 1], 1024_u64 * 1024)
    } else if limit.ends_with(['K', 'k']) {
        (&limit[..limit.len() - 1], 1024_u64)
    } else {
        (limit, 1_u64)
    };
    let value: f64 = number.parse().ok()?;
    (value.is_finite() && value > 0.0).then(|| (value * unit as f64).round() as u64)
}

pub(super) fn throttle_transfer(
    window_start: Instant,
    window_bytes: u64,
    bytes_per_second: u64,
) -> (Instant, u64) {
    let elapsed = window_start.elapsed();
    let expected = Duration::from_secs_f64(window_bytes as f64 / bytes_per_second as f64);
    if expected > elapsed {
        thread::sleep(expected - elapsed);
    }
    if window_start.elapsed() >= Duration::from_secs(1) {
        (Instant::now(), 0)
    } else {
        (window_start, window_bytes)
    }
}

pub(super) fn acquire_download_slot() {
    let max = max_concurrent_downloads() as u32;
    let semaphore = DOWNLOAD_SEMAPHORE.get_or_init(|| Arc::new((Mutex::new(0), Condvar::new())));
    let (lock, condition) = semaphore.as_ref();
    let mut active = lock.lock().unwrap();
    while *active >= max {
        active = condition.wait(active).unwrap();
    }
    *active += 1;
}

pub(super) fn release_download_slot() {
    if let Some(semaphore) = DOWNLOAD_SEMAPHORE.get() {
        let (lock, condition) = semaphore.as_ref();
        let mut active = lock.lock().unwrap();
        *active = active.saturating_sub(1);
        condition.notify_one();
    }
}

/// Returns capacity on every exit, including a worker panic.
pub(super) struct DownloadSlot;
impl DownloadSlot {
    pub(super) fn acquire() -> Self {
        acquire_download_slot();
        Self
    }
}
impl Drop for DownloadSlot {
    fn drop(&mut self) {
        release_download_slot();
    }
}

pub(super) fn register_controller(
    store: &TaskControllerStore,
    task_id: &str,
    controller: Arc<TaskController>,
) {
    store
        .lock()
        .unwrap()
        .insert(task_id.to_string(), controller);
}

pub(super) fn unregister_controller(store: &TaskControllerStore, task_id: &str) {
    store.lock().unwrap().remove(task_id);
}

pub fn pause_task(
    tasks: task_store::TaskStore,
    controllers: TaskControllerStore,
    task_id: &str,
) -> Result<DownloadTask, String> {
    let controller = find_controller(&controllers, task_id, "暂停")?;
    if !controller.supports_pause {
        return Err("当前任务暂不支持暂停。".to_string());
    }
    controller.request_pause();
    task_store::mutate_task(&tasks, task_id, |task| {
        task.status = "paused".to_string();
        task.message = Some("已暂停，可以继续或取消。".to_string());
    })
}

pub fn resume_task(
    tasks: task_store::TaskStore,
    controllers: TaskControllerStore,
    task_id: &str,
) -> Result<DownloadTask, String> {
    let controller = find_controller(&controllers, task_id, "继续")?;
    if !controller.supports_pause {
        return Err("当前任务暂不支持继续。".to_string());
    }
    controller.request_resume();
    task_store::mutate_task(&tasks, task_id, |task| {
        task.status = "downloading".to_string();
        task.message = Some("继续下载...".to_string());
    })
}

pub fn cancel_task(
    tasks: task_store::TaskStore,
    controllers: TaskControllerStore,
    task_id: &str,
) -> Result<DownloadTask, String> {
    let controller = find_controller(&controllers, task_id, "取消")?;
    if !controller.supports_cancel {
        return Err("当前任务暂不支持取消。".to_string());
    }
    controller.request_cancel();
    task_store::mutate_task(&tasks, task_id, |task| {
        task.message = Some("正在取消...".to_string());
    })
}

fn find_controller(
    store: &TaskControllerStore,
    task_id: &str,
    action: &str,
) -> Result<Arc<TaskController>, String> {
    store
        .lock()
        .unwrap()
        .get(task_id)
        .cloned()
        .ok_or_else(|| format!("当前任务已经结束，无法{action}。"))
}

// A reservation outlives terminal-state writes and controller removal, so an old
// worker cannot overwrite a new retry or write to its output files.
static ACTIVE_DOWNLOADS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub(super) struct DownloadReservation {
    keys: Vec<String>,
    store: TaskControllerStore,
    task_id: String,
    controller: Arc<TaskController>,
}

impl DownloadReservation {
    pub(super) fn reserve_output(&mut self, output_key: String) -> Result<(), String> {
        let key = format!("output:{output_key}");
        if self.keys.contains(&key) {
            return Ok(());
        }
        let mut active = ACTIVE_DOWNLOADS.get().unwrap().lock().unwrap();
        if !active.insert(key.clone()) {
            return Err("输出位置已有活跃下载，请等待任务结束后重试。".into());
        }
        self.keys.push(key);
        Ok(())
    }

    pub(super) fn acquire(
        store: &TaskControllerStore,
        task_id: &str,
        output_key: String,
        controller: Arc<TaskController>,
    ) -> Result<Self, String> {
        let keys = vec![format!("task:{task_id}"), format!("output:{output_key}")];
        let mut active = ACTIVE_DOWNLOADS
            .get_or_init(Default::default)
            .lock()
            .unwrap();
        if keys.iter().any(|key| active.contains(key)) {
            return Err("同一作品或输出位置已有活跃下载，请等待任务结束后重试。".into());
        }
        active.extend(keys.iter().cloned());
        register_controller(store, task_id, Arc::clone(&controller));
        Ok(Self {
            keys,
            store: Arc::clone(store),
            task_id: task_id.into(),
            controller,
        })
    }
}

impl Drop for DownloadReservation {
    fn drop(&mut self) {
        let mut controllers = self.store.lock().unwrap_or_else(|e| e.into_inner());
        if controllers
            .get(&self.task_id)
            .is_some_and(|current| Arc::ptr_eq(current, &self.controller))
        {
            controllers.remove(&self.task_id);
        }
        drop(controllers);
        let mut active = ACTIVE_DOWNLOADS
            .get()
            .unwrap()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for key in &self.keys {
            active.remove(key);
        }
    }
}

#[cfg(test)]
mod reservation_tests {
    use super::*;

    #[test]
    fn duplicate_task_and_output_cannot_replace_active_controller() {
        let store = new_task_controller_store();
        let controller = Arc::new(TaskController::new(false, true));
        let first = DownloadReservation::acquire(
            &store,
            "reservation-test-1",
            "reservation-output-1".into(),
            Arc::clone(&controller),
        )
        .unwrap();
        assert!(DownloadReservation::acquire(
            &store,
            "reservation-test-1",
            "another-output".into(),
            Arc::new(TaskController::new(false, true))
        )
        .is_err());
        assert!(DownloadReservation::acquire(
            &store,
            "different-task",
            "reservation-output-1".into(),
            Arc::new(TaskController::new(false, true))
        )
        .is_err());
        assert!(Arc::ptr_eq(
            store.lock().unwrap().get("reservation-test-1").unwrap(),
            &controller
        ));
        // Workers currently unregister immediately before writing terminal state.
        // The reservation must still forbid a retry throughout that interval.
        unregister_controller(&store, "reservation-test-1");
        assert!(DownloadReservation::acquire(
            &store,
            "reservation-test-1",
            "reservation-output-1".into(),
            Arc::clone(&controller)
        )
        .is_err());
        drop(first);
        let retry = DownloadReservation::acquire(
            &store,
            "reservation-test-1",
            "reservation-output-1".into(),
            controller,
        )
        .unwrap();
        drop(retry);
        assert!(store.lock().unwrap().is_empty());
    }

    #[test]
    fn reserves_the_allocated_bundle_path_against_other_retries() {
        let store = new_task_controller_store();
        let controller = Arc::new(TaskController::new(false, true));
        let mut first = DownloadReservation::acquire(
            &store,
            "allocated-first",
            "allocated-title".into(),
            Arc::clone(&controller),
        )
        .unwrap();
        first.reserve_output("allocated-title (2)".into()).unwrap();
        assert!(DownloadReservation::acquire(
            &store,
            "allocated-retry",
            "allocated-title (2)".into(),
            Arc::clone(&controller)
        )
        .is_err());
        drop(first);
        assert!(DownloadReservation::acquire(
            &store,
            "allocated-retry",
            "allocated-title (2)".into(),
            controller
        )
        .is_ok());
    }

    #[test]
    fn racing_requests_have_exactly_one_owner_and_release_on_unwind() {
        let store = new_task_controller_store();
        let start = Arc::new(std::sync::Barrier::new(8));
        let reserved = Arc::new(std::sync::Barrier::new(8));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                let start = Arc::clone(&start);
                let reserved = Arc::clone(&reserved);
                thread::spawn(move || {
                    start.wait();
                    let reservation = DownloadReservation::acquire(
                        &store,
                        "reservation-race",
                        "reservation-race-output".into(),
                        Arc::new(TaskController::new(false, true)),
                    );
                    reserved.wait();
                    reservation.is_ok()
                })
            })
            .collect();
        assert_eq!(
            workers
                .into_iter()
                .map(|worker| usize::from(worker.join().unwrap()))
                .sum::<usize>(),
            1
        );
        let result = std::panic::catch_unwind(|| {
            let _reservation = DownloadReservation::acquire(
                &store,
                "reservation-race",
                "reservation-race-output".into(),
                Arc::new(TaskController::new(false, true)),
            )
            .unwrap();
            panic!("simulated worker failure");
        });
        assert!(result.is_err());
        assert!(store.lock().unwrap().is_empty());
        assert!(DownloadReservation::acquire(
            &store,
            "reservation-race",
            "reservation-race-output".into(),
            Arc::new(TaskController::new(false, true))
        )
        .is_ok());
    }
}
