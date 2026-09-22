//! Atomic JSON replacement and preservation of unreadable saved state.
use serde::{de::DeserializeOwned, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

static BLOCKED_LOADS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

// 同一目标文件的连续保存失败次数：用于失败日志节流，避免 2s 重试刷屏。
static SAVE_FAILURES: OnceLock<Mutex<HashMap<PathBuf, u64>>> = OnceLock::new();

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

fn private_file(path: &Path, suffix: &str) -> io::Result<(PathBuf, File)> {
    for _ in 0..100 {
        let id = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let name = format!(
            "{}.{}.{}.{}",
            path.file_name().unwrap_or_default().to_string_lossy(),
            std::process::id(),
            id,
            suffix
        );
        let temporary = path.with_file_name(name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "无法创建唯一临时文件",
    ))
}

/// 若文件名符合本模块的 `{目标名}.{pid}.{序号}.{tmp|corrupt}` 命名规则，
/// 返回 (pid, 序号, 是否临时文件)；否则返回 None（可能是用户文件，绝不动它）。
fn sibling_kind(name: &str, prefix: &str) -> Option<(u32, u64, bool)> {
    let rest = name.strip_prefix(prefix)?;
    let (stem, is_tmp) = if let Some(stem) = rest.strip_suffix(".tmp") {
        (stem, true)
    } else if let Some(stem) = rest.strip_suffix(".corrupt") {
        (stem, false)
    } else {
        return None;
    };
    let (pid, id) = stem.split_once('.')?;
    Some((pid.parse().ok()?, id.parse().ok()?, is_tmp))
}

/// 清理本模块为同一目标文件遗留的陈旧文件：崩溃残留的 `.tmp` 全部删除；
/// `.corrupt` 隔离备份只保留最新一份，避免无界堆积又不至于删掉最近的现场。
/// 跳过当前进程 pid 的文件（可能正在写入），全部操作尽力而为。
fn clean_stale_siblings(path: &Path) {
    let (Some(parent), Some(file_name)) = (path.parent(), path.file_name()) else {
        return;
    };
    let prefix = format!("{}.", file_name.to_string_lossy());
    let current_pid = std::process::id();
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    let mut stale = Vec::new();
    let mut newest_corrupt: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some((pid, _, is_tmp)) = sibling_kind(&name.to_string_lossy(), &prefix) else {
            continue;
        };
        if pid == current_pid {
            continue;
        }
        let sibling = entry.path();
        if is_tmp {
            stale.push(sibling);
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        if let Some((newest, old)) = &mut newest_corrupt {
            if *newest >= modified {
                stale.push(sibling);
            } else {
                stale.push(std::mem::replace(old, sibling));
                *newest = modified;
            }
        } else {
            newest_corrupt = Some((modified, sibling));
        }
    }
    for file in stale {
        let _ = fs::remove_file(file);
    }
}

pub fn load_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    clean_stale_siblings(path);
    let raw = match fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return T::default(),
        Err(error) => {
            BLOCKED_LOADS
                .get_or_init(Default::default)
                .lock()
                .unwrap()
                .insert(path.to_path_buf());
            eprintln!("读取保存状态失败（{}）：{error}", path.display());
            return T::default();
        }
    };
    match serde_json::from_slice(&raw) {
        Ok(value) => {
            BLOCKED_LOADS
                .get_or_init(Default::default)
                .lock()
                .unwrap()
                .remove(path);
            value
        }
        Err(error) => {
            // Preserve the exact original before allowing a new empty state to be saved.
            let backup = (|| -> Result<PathBuf, String> {
                let (backup, file) = private_file(path, "corrupt").map_err(|e| e.to_string())?;
                drop(file);
                if let Err(error) = crate::auth::atomic_replace(path, &backup) {
                    let _ = fs::remove_file(&backup);
                    return Err(error);
                }
                Ok(backup)
            })();
            match backup {
                Ok(backup) => eprintln!(
                    "保存状态损坏（{error}），原文件已保留：{}",
                    backup.display()
                ),
                Err(reason) => eprintln!(
                    "保存状态损坏（{error}），无法隔离原文件，后续保存将拒绝覆盖：{reason}"
                ),
            }
            T::default()
        }
    }
}

pub fn save_json<T: Serialize + DeserializeOwned>(path: &Path, value: &T) -> Result<(), String> {
    let result = (|| -> Result<(), String> {
        if BLOCKED_LOADS
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .contains(path)
        {
            return Err("原保存文件未成功加载，拒绝覆盖；请重启应用后重试".into());
        }
        // A failed quarantine/read must never turn into silent data replacement.
        match fs::read(path) {
            Ok(raw) => {
                serde_json::from_slice::<T>(&raw)
                    .map_err(|_| "原保存文件损坏，拒绝覆盖".to_string())?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        let content = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        clean_stale_siblings(path);
        let (temporary, mut file) = private_file(path, "tmp").map_err(|e| e.to_string())?;
        let written = (|| -> Result<(), String> {
            file.write_all(&content).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            drop(file);
            crate::auth::atomic_replace(&temporary, path)
        })();
        if written.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        written
    })();
    match &result {
        Err(error) => {
            // 同一目标的连续失败只记首次：task_store 每 2s 重试，逐次记录会刷屏
            let mut failures = SAVE_FAILURES.get_or_init(Default::default).lock().unwrap();
            let count = failures.entry(path.to_path_buf()).or_insert(0);
            *count += 1;
            if *count == 1 {
                eprintln!("保存状态失败（{}）：{error}", path.display());
            }
        }
        Ok(()) => {
            let recovered = SAVE_FAILURES
                .get_or_init(Default::default)
                .lock()
                .unwrap()
                .remove(path);
            if let Some(count) = recovered {
                eprintln!(
                    "保存状态已恢复（{}），此前连续失败 {count} 次",
                    path.display()
                );
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn folder() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "streamverse-persistence-{}-{}",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
    #[test]
    fn stale_siblings_are_cleaned_without_touching_user_files() {
        let root = folder();
        let path = root.join("state.json");
        let other_pid = std::process::id().wrapping_add(1);
        let stale_tmp = root.join(format!("state.json.{other_pid}.1.tmp"));
        let old_corrupt = root.join(format!("state.json.{other_pid}.2.corrupt"));
        let new_corrupt = root.join(format!("state.json.{other_pid}.3.corrupt"));
        fs::write(&stale_tmp, b"partial").unwrap();
        fs::write(&old_corrupt, b"old").unwrap();
        fs::write(&new_corrupt, b"new").unwrap();
        // 用显式更旧的修改时间标记“较老”的隔离备份，避免依赖创建顺序
        File::options()
            .write(true)
            .open(&old_corrupt)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(100))
            .unwrap();
        // 当前进程 pid 的文件可能正在写入，不能清；命名不匹配的一律视为用户文件
        let inflight = root.join(format!("state.json.{}.9.tmp", std::process::id()));
        let user_files = [
            root.join("state.json.bak"),
            root.join("state.json.abc.1.tmp"),
            root.join("state.json.123.1.tmp.bak"),
            root.join("other.json.123.1.tmp"),
        ];
        fs::write(&inflight, b"inflight").unwrap();
        for file in &user_files {
            fs::write(file, b"user").unwrap();
        }

        clean_stale_siblings(&path);

        assert!(!stale_tmp.exists());
        assert!(!old_corrupt.exists());
        assert_eq!(fs::read(&new_corrupt).unwrap(), b"new");
        assert!(inflight.exists());
        for file in &user_files {
            assert!(file.exists(), "误删用户文件：{}", file.display());
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn repeated_save_failures_are_counted_once_and_cleared_on_recovery() {
        let root = folder();
        let path = root.join("state.json");
        // 目标是目录时读取失败，保存持续报错
        fs::create_dir(&path).unwrap();
        assert!(save_json(&path, &vec![1_u32]).is_err());
        assert!(save_json(&path, &vec![1_u32]).is_err());
        {
            let failures = SAVE_FAILURES.get_or_init(Default::default).lock().unwrap();
            assert_eq!(failures.get(&path), Some(&2));
        }
        fs::remove_dir(&path).unwrap();
        save_json(&path, &vec![1_u32]).unwrap();
        {
            let failures = SAVE_FAILURES.get_or_init(Default::default).lock().unwrap();
            assert!(!failures.contains_key(&path));
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn replace_existing_and_preserve_corrupt_original() {
        let root = folder();
        let path = root.join("state.json");
        save_json(&path, &vec![1_u32]).unwrap();
        save_json(&path, &vec![2_u32, 3]).unwrap();
        assert_eq!(load_json::<Vec<u32>>(&path), vec![2, 3]);
        fs::write(&path, b"{broken").unwrap();
        assert!(save_json(&path, &vec![4_u32]).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{broken");
        assert!(load_json::<Vec<u32>>(&path).is_empty());
        let backup = fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|e| e == "corrupt"))
            .unwrap();
        assert_eq!(fs::read(backup).unwrap(), b"{broken");
        save_json(&path, &vec![4_u32]).unwrap();
        assert_eq!(load_json::<Vec<u32>>(&path), vec![4]);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn concurrent_replacements_never_publish_partial_json() {
        let root = folder();
        let path = root.join("state.json");
        save_json(&path, &vec![0_u32]).unwrap();
        std::thread::scope(|scope| {
            for n in 1..5 {
                let path = &path;
                scope.spawn(move || {
                    for _ in 0..10 {
                        save_json(path, &vec![n; 1000]).unwrap();
                    }
                });
            }
            for _ in 0..100 {
                let _: Vec<u32> = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            }
        });
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_initial_read_cannot_overwrite_recovered_file() {
        let root = folder();
        let path = root.join("state.json");
        fs::create_dir(&path).unwrap();
        assert!(load_json::<Vec<u32>>(&path).is_empty());
        fs::remove_dir(&path).unwrap();
        fs::write(&path, b"[42]").unwrap();
        assert!(save_json(&path, &vec![1_u32]).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"[42]");
        assert_eq!(load_json::<Vec<u32>>(&path), vec![42]);
        save_json(&path, &vec![42_u32, 1]).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
