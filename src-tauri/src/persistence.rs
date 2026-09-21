//! Atomic JSON replacement and preservation of unreadable saved state.
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

static BLOCKED_LOADS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

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

pub fn load_json<T: DeserializeOwned + Default>(path: &Path) -> T {
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
    if let Err(error) = &result {
        eprintln!("保存状态失败（{}）：{error}", path.display());
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
