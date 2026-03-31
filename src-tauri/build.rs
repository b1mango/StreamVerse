use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=STREAMVERSE_YTDLP_PATH");
    println!("cargo:rerun-if-env-changed=PATH");

    if let Err(error) = prepare_bundled_ytdlp() {
        panic!("failed to prepare bundled yt-dlp: {error}");
    }

    tauri_build::build()
}

fn prepare_bundled_ytdlp() -> Result<(), String> {
    let target = generated_resource_dir().join(ytdlp_binary_name());

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create generated resource dir failed: {error}"))?;
    }

    if let Ok(path) = env::var("STREAMVERSE_YTDLP_PATH") {
        let source = PathBuf::from(path);
        if !source.is_file() {
            return Err(format!(
                "STREAMVERSE_YTDLP_PATH does not exist: {}",
                source.display()
            ));
        }
        fs::copy(&source, &target)
            .map_err(|error| format!("copy yt-dlp into generated resources failed: {error}"))?;
    } else if !is_portable_ytdlp_binary(&target)? {
        download_standalone_ytdlp(&target)?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755))
            .map_err(|error| format!("set yt-dlp executable bit failed: {error}"))?;
    }

    Ok(())
}

fn generated_resource_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("gen")
        .join("resources")
        .join("download-engine")
        .join("bin")
}

fn ytdlp_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

fn ytdlp_download_url() -> &'static str {
    if cfg!(target_os = "windows") {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe"
    } else if cfg!(target_os = "macos") {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos"
    } else {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux"
    }
}

fn download_standalone_ytdlp(target: &Path) -> Result<(), String> {
    let status = Command::new("curl")
        .arg("-L")
        .arg("--fail")
        .arg("--retry")
        .arg("3")
        .arg("--retry-delay")
        .arg("2")
        .arg("-A")
        .arg(format!("StreamVerse/{}", env!("CARGO_PKG_VERSION")))
        .arg("-o")
        .arg(target)
        .arg(ytdlp_download_url())
        .status()
        .map_err(|error| format!("download yt-dlp failed: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("download yt-dlp failed via curl".to_string())
    }
}

fn is_portable_ytdlp_binary(path: &Path) -> Result<bool, String> {
    if !path.is_file() {
        return Ok(false);
    }

    #[cfg(target_os = "windows")]
    {
        return Ok(true);
    }

    let mut file =
        fs::File::open(path).map_err(|error| format!("open generated yt-dlp failed: {error}"))?;
    let mut prefix = [0u8; 2];
    let read = file
        .read(&mut prefix)
        .map_err(|error| format!("read generated yt-dlp failed: {error}"))?;
    if read >= 2 && prefix == [b'#', b'!'] {
        return Ok(false);
    }
    Ok(true)
}
