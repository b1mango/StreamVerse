use crate::{pack_registry, settings};
use reqwest::blocking::Client;
use reqwest::header::ACCEPT_ENCODING;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::ZipArchive;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const PACKS_DIR_ENV: &str = "STREAMVERSE_PACKS_DIR";
const REGISTRY_PATH_ENV: &str = "STREAMVERSE_PACK_REGISTRY_PATH";
const REGISTRY_URL_ENV: &str = "STREAMVERSE_PACK_REGISTRY_URL";
const DEFAULT_REGISTRY_URL: &str =
    "https://github.com/b1mango/StreamVerse/releases/latest/download/plugins.json";

#[derive(Deserialize)]
struct PackRegistryManifest {
    #[serde(default)]
    packs: Vec<RegistryPack>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryPack {
    id: String,
    binary_name: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    resource_only: bool,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    size_bytes: Option<u64>,
    #[serde(default)]
    sha256: Option<String>,
    source: RegistryPackSource,
}

#[derive(Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum RegistryPackSource {
    LocalBuild,
    Url { url: String },
}

enum InstallArtifactKind {
    Binary,
    ZipBundle,
    LocalResourceBundle,
}

enum InstallArtifact {
    Existing(PathBuf, InstallArtifactKind),
    Temporary(PathBuf, InstallArtifactKind),
}

#[derive(Clone)]
pub struct ModulePackRuntimeInfo {
    pub pack_id: String,
    pub current_version: Option<String>,
    pub latest_version: Option<String>,
    pub size_bytes: Option<u64>,
    pub source_kind: String,
    pub update_available: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstalledPackManifest {
    id: String,
    binary_name: String,
    version: Option<String>,
    sha256: Option<String>,
    source_kind: String,
    installed_at: u64,
}

pub fn refresh_installed_state(settings: &mut settings::AppSettings) -> bool {
    let mut changed = false;

    for module_id in settings::MODULE_IDS {
        let installed = is_module_installed(module_id);
        let module = settings.modules.entry(module_id.to_string()).or_default();

        if module.installed != installed {
            module.installed = installed;
            changed = true;
        }

        if !installed && module.enabled {
            module.enabled = false;
            changed = true;
        }
    }

    changed
}

pub fn install_pack_for_module(
    module_id: &str,
    settings: &mut settings::AppSettings,
) -> Result<(), String> {
    let normalized_id = settings::normalize_module_id(module_id)?;
    for dependency_id in pack_registry::shared_dependencies_for_module(normalized_id) {
        install_shared_pack(dependency_id)?;
    }
    let pack = pack_registry::local_pack_for_module(normalized_id)
        .ok_or_else(|| format!("模块 {normalized_id} 还没有对应的本地 pack。"))?;
    let registry_entry = load_registry_pack(pack.id)
        .unwrap_or_else(|| default_registry_pack(pack.id, pack.binary_name));
    let artifact = resolve_install_artifact(&registry_entry)?;
    let install_dir = installed_pack_dir(pack.id);

    install_artifact(
        &artifact,
        &install_dir,
        pack.id,
        pack.binary_name,
        registry_entry.sha256.as_deref(),
    )?;
    write_installed_manifest(&registry_entry)?;
    cleanup_artifact(artifact);

    for related_module_id in pack.module_ids {
        let module = settings
            .modules
            .entry((*related_module_id).to_string())
            .or_default();
        module.installed = true;
        module.enabled = true;
    }

    Ok(())
}

pub fn update_pack_for_module(
    module_id: &str,
    settings: &mut settings::AppSettings,
) -> Result<(), String> {
    install_pack_for_module(module_id, settings)
}

pub fn uninstall_pack_for_module(
    module_id: &str,
    settings: &mut settings::AppSettings,
) -> Result<(), String> {
    let normalized_id = settings::normalize_module_id(module_id)?;
    let pack = pack_registry::local_pack_for_module(normalized_id)
        .ok_or_else(|| format!("模块 {normalized_id} 还没有对应的本地 pack。"))?;
    let install_dir = installed_pack_dir(pack.id);

    if install_dir.exists() {
        fs::remove_dir_all(&install_dir).map_err(|error| format!("卸载本地 pack 失败：{error}"))?;
    }

    for related_module_id in pack.module_ids {
        let module = settings
            .modules
            .entry((*related_module_id).to_string())
            .or_default();
        module.installed = false;
        module.enabled = false;
    }

    cleanup_unused_shared_packs(settings)?;

    Ok(())
}

pub fn is_module_installed(module_id: &str) -> bool {
    pack_registry::local_pack_for_module(module_id)
        .map(|pack| resolve_installed_binary(pack.binary_name).is_some())
        .unwrap_or(false)
}

pub fn resolve_installed_binary(binary_name: &str) -> Option<PathBuf> {
    let pack = pack_registry::local_pack_for_binary(binary_name)?;
    let path = installed_binary_path(pack.id, pack.binary_name);
    if path.is_file() {
        Some(path)
    } else {
        locate_pack_source_binary(pack.binary_name)
    }
}

pub fn ensure_media_engine_installed() -> Result<Option<PathBuf>, String> {
    if let Some(path) =
        resolve_shared_pack_file("media-engine", Path::new("bin").join(ffmpeg_binary_name()))
    {
        return Ok(Some(path));
    }

    ffmpeg_source_path().map(Some)
}

pub fn ensure_download_engine_installed() -> Result<Option<PathBuf>, String> {
    if let Some(path) = resolve_shared_pack_file(
        "download-engine",
        Path::new("bin").join(ytdlp_binary_name()),
    ) {
        if can_execute_ytdlp(&path) {
            return Ok(Some(path));
        }
    }

    let bundled = ytdlp_source_path()?;
    if can_execute_ytdlp(&bundled) {
        Ok(Some(bundled))
    } else {
        Err("应用内置 yt-dlp 不可用，请重新安装应用后再试。".to_string())
    }
}

pub fn resolve_shared_pack_file(pack_id: &str, relative_path: PathBuf) -> Option<PathBuf> {
    let installed = installed_pack_dir(pack_id).join(&relative_path);
    if installed.exists() {
        return Some(installed);
    }

    if let Some(path) = packaged_pack_resource_root(pack_id)
        .map(|root| root.join(&relative_path))
        .filter(|path| path.exists())
    {
        return Some(path);
    }

    match pack_id {
        "download-engine" => ytdlp_source_path().ok(),
        "media-engine" => ffmpeg_source_path().ok(),
        _ => None,
    }
}

pub fn module_runtime_info(module_id: &str) -> Option<ModulePackRuntimeInfo> {
    let pack = pack_registry::local_pack_for_module(module_id)?;
    let registry_pack = load_registry_pack(pack.id)
        .unwrap_or_else(|| default_registry_pack(pack.id, pack.binary_name));
    let dependency_size = pack_registry::shared_dependencies_for_module(module_id)
        .iter()
        .filter_map(|dependency_id| shared_pack_size(dependency_id))
        .sum::<u64>();
    let installed_manifest = read_installed_manifest(pack.id);
    let installed = resolve_installed_binary(pack.binary_name).is_some();
    let current_version = if installed {
        installed_manifest
            .as_ref()
            .and_then(|manifest| manifest.version.clone())
            .or_else(|| registry_pack.version.clone())
    } else {
        None
    };
    let latest_version = registry_pack.version.clone();
    let update_available = matches!(
        (current_version.as_deref(), latest_version.as_deref()),
        (Some(current), Some(latest)) if current != latest
    );

    Some(ModulePackRuntimeInfo {
        pack_id: pack.id.to_string(),
        current_version,
        latest_version,
        size_bytes: registry_pack
            .size_bytes
            .or_else(|| registry_pack_size(&registry_pack))
            .or_else(|| resolve_installed_binary(pack.binary_name).and_then(file_size))
            .map(|value| value + dependency_size),
        source_kind: registry_pack.source.kind_label().to_string(),
        update_available,
    })
}

fn load_registry_pack(pack_id: &str) -> Option<RegistryPack> {
    let manifest = load_registry_manifest().ok()?;
    let current_target = current_target();

    manifest.packs.into_iter().find(|pack| {
        pack.id == pack_id
            && pack
                .target
                .as_deref()
                .map(|value| value == current_target)
                .unwrap_or(true)
    })
}

fn load_registry_manifest() -> Result<PackRegistryManifest, String> {
    if let Ok(url) = env::var(REGISTRY_URL_ENV) {
        if !url.trim().is_empty() {
            return load_registry_manifest_from_url(&url);
        }
    }

    let path = registry_path();
    if path.is_file() {
        let raw = fs::read_to_string(&path)
            .map_err(|error| format!("读取 pack 注册表失败（{}）：{error}", path.display()))?;
        return serde_json::from_str::<PackRegistryManifest>(&raw)
            .map_err(|error| format!("解析 pack 注册表失败：{error}"));
    }

    load_registry_manifest_from_url(DEFAULT_REGISTRY_URL).or_else(|remote_error| {
        let raw = fs::read_to_string(&path)
            .map_err(|error| format!("读取 pack 注册表失败（{}）：{error}", path.display()))?;
        serde_json::from_str::<PackRegistryManifest>(&raw)
            .map_err(|error| format!("{remote_error}\n\n解析本地 pack 注册表失败：{error}"))
    })
}

fn load_registry_manifest_from_url(url: &str) -> Result<PackRegistryManifest, String> {
    if let Some(path) = url.strip_prefix("file://") {
        let raw = fs::read_to_string(path)
            .map_err(|error| format!("读取远程注册表文件失败（{path}）：{error}"))?;
        return serde_json::from_str::<PackRegistryManifest>(&raw)
            .map_err(|error| format!("解析远程注册表失败：{error}"));
    }

    let client = Client::builder()
        .build()
        .map_err(|error| format!("初始化注册表客户端失败：{error}"))?;
    let response = client
        .get(url)
        .header("User-Agent", "StreamVerse/0.1.0")
        .send()
        .map_err(|error| format!("下载 pack 注册表失败：{error}"))?;

    if !response.status().is_success() {
        return Err(format!("下载 pack 注册表失败：HTTP {}", response.status()));
    }

    let raw = response
        .text()
        .map_err(|error| format!("读取 pack 注册表内容失败：{error}"))?;
    serde_json::from_str::<PackRegistryManifest>(&raw)
        .map_err(|error| format!("解析远程注册表失败：{error}"))
}

fn resolve_install_artifact(registry_pack: &RegistryPack) -> Result<InstallArtifact, String> {
    match &registry_pack.source {
        RegistryPackSource::LocalBuild if registry_pack.resource_only => Ok(
            InstallArtifact::Existing(workspace_root(), InstallArtifactKind::LocalResourceBundle),
        ),
        RegistryPackSource::LocalBuild => locate_pack_source_binary(&registry_pack.binary_name)
            .map(|path| InstallArtifact::Existing(path, InstallArtifactKind::Binary))
            .ok_or_else(|| {
                format!(
                    "未找到可安装的本地 pack：{}。请先执行 `cargo build --bins`。",
                    registry_pack.binary_name
                )
            }),
        RegistryPackSource::Url { url } => {
            download_install_artifact(url, &registry_pack.binary_name)
        }
    }
}

fn install_artifact(
    artifact: &InstallArtifact,
    install_dir: &Path,
    pack_id: &str,
    binary_name: &str,
    expected_sha256: Option<&str>,
) -> Result<(), String> {
    let (source_path, kind) = match artifact {
        InstallArtifact::Existing(path, kind) | InstallArtifact::Temporary(path, kind) => {
            (path, kind)
        }
    };

    if let Some(expected) = expected_sha256 {
        verify_sha256(source_path, expected)?;
    }

    if install_dir.exists() {
        fs::remove_dir_all(install_dir)
            .map_err(|error| format!("清理旧 pack 目录失败：{error}"))?;
    }

    match kind {
        InstallArtifactKind::Binary => {
            let target_path = installed_binary_path(pack_id, binary_name);
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("创建 pack 目录失败：{error}"))?;
            }

            fs::copy(source_path, &target_path)
                .map_err(|error| format!("安装本地 pack 失败：{error}"))?;
            make_executable(&target_path)?;
            sync_local_pack_resources(pack_id, install_dir, false)?;
        }
        InstallArtifactKind::ZipBundle => {
            fs::create_dir_all(install_dir)
                .map_err(|error| format!("创建 pack 目录失败：{error}"))?;
            extract_zip_bundle(source_path, install_dir)?;
            if !load_registry_pack(pack_id)
                .as_ref()
                .map(|pack| pack.resource_only)
                .unwrap_or(false)
            {
                let bin_path = installed_binary_path(pack_id, binary_name);
                if !bin_path.is_file() {
                    return Err(format!("pack 压缩包缺少可执行文件：{}", bin_path.display()));
                }
                make_executable(&bin_path)?;
            }
        }
        InstallArtifactKind::LocalResourceBundle => {
            fs::create_dir_all(install_dir)
                .map_err(|error| format!("创建 pack 目录失败：{error}"))?;
            sync_local_pack_resources(pack_id, install_dir, true)?;
        }
    }

    Ok(())
}

fn write_installed_manifest(registry_pack: &RegistryPack) -> Result<(), String> {
    let manifest = InstalledPackManifest {
        id: registry_pack.id.clone(),
        binary_name: registry_pack.binary_name.clone(),
        version: registry_pack.version.clone(),
        sha256: registry_pack.sha256.clone(),
        source_kind: registry_pack.source.kind_label().to_string(),
        installed_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or(0),
    };
    let manifest_path = installed_manifest_path(&registry_pack.id);
    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建 pack 元数据目录失败：{error}"))?;
    }
    let raw = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("序列化 pack 元数据失败：{error}"))?;
    fs::write(&manifest_path, raw).map_err(|error| format!("写入 pack 元数据失败：{error}"))
}

fn install_shared_pack(pack_id: &str) -> Result<(), String> {
    let shared = pack_registry::shared_pack_for_id(pack_id)
        .ok_or_else(|| format!("未识别的共享依赖包：{pack_id}"))?;
    let registry_entry =
        load_registry_pack(shared.id).unwrap_or_else(|| default_shared_registry_pack(shared.id));
    let artifact = resolve_install_artifact(&registry_entry)?;
    let install_dir = installed_pack_dir(shared.id);
    install_artifact(
        &artifact,
        &install_dir,
        shared.id,
        &registry_entry.binary_name,
        registry_entry.sha256.as_deref(),
    )?;
    write_installed_manifest(&registry_entry)?;
    cleanup_artifact(artifact);
    Ok(())
}

fn cleanup_unused_shared_packs(settings: &settings::AppSettings) -> Result<(), String> {
    for pack_id in ["browser-bridge", "download-engine", "media-engine"] {
        let still_needed = settings.modules.iter().any(|(module_id, module)| {
            module.installed
                && module.enabled
                && pack_registry::shared_dependencies_for_module(module_id).contains(&pack_id)
        });

        if !still_needed {
            let install_dir = installed_pack_dir(pack_id);
            if install_dir.exists() {
                fs::remove_dir_all(&install_dir)
                    .map_err(|error| format!("清理共享依赖包失败：{error}"))?;
            }
        }
    }

    Ok(())
}

fn make_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let permissions = fs::Permissions::from_mode(0o755);
        fs::set_permissions(path, permissions)
            .map_err(|error| format!("设置 pack 执行权限失败：{error}"))?;
    }

    #[cfg(not(unix))]
    {
        let _ = path;
    }

    Ok(())
}

fn extract_zip_bundle(zip_path: &Path, install_dir: &Path) -> Result<(), String> {
    let file =
        fs::File::open(zip_path).map_err(|error| format!("读取 pack 压缩包失败：{error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("解析 pack 压缩包失败：{error}"))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("读取 pack 压缩包条目失败：{error}"))?;
        let Some(path) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            continue;
        };
        let destination = install_dir.join(path);

        if entry.is_dir() {
            fs::create_dir_all(&destination)
                .map_err(|error| format!("创建 pack 目录失败：{error}"))?;
            continue;
        }

        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("创建 pack 目录失败：{error}"))?;
        }

        let mut output = fs::File::create(&destination)
            .map_err(|error| format!("写入 pack 文件失败：{error}"))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|error| format!("解压 pack 文件失败：{error}"))?;
    }

    Ok(())
}

fn sync_local_pack_resources(
    pack_id: &str,
    install_dir: &Path,
    resource_only: bool,
) -> Result<(), String> {
    let resource_root = packaged_pack_resource_root(pack_id).unwrap_or_else(|| workspace_root());

    match pack_id {
        "douyin-pack" => {
            copy_optional_file(
                &resource_root.join("scripts").join("douyin_bridge.py"),
                &install_dir.join("scripts").join("douyin_bridge.py"),
            )?;
            copy_optional_dir(
                &resource_root.join("vendor").join("douyin_api"),
                &install_dir.join("vendor").join("douyin_api"),
            )?;
        }
        "bilibili-pack" => {
            copy_optional_file(
                &resource_root
                    .join("scripts")
                    .join("bilibili_profile_bridge.py"),
                &install_dir
                    .join("scripts")
                    .join("bilibili_profile_bridge.py"),
            )?;
        }
        "browser-bridge" => {
            copy_optional_file(
                &resource_root.join("requirements-douyin-helper.txt"),
                &install_dir.join("requirements-douyin-helper.txt"),
            )?;
            copy_optional_file(
                &resource_root
                    .join("scripts")
                    .join("profile_browser_scan.py"),
                &install_dir.join("scripts").join("profile_browser_scan.py"),
            )?;
        }
        "download-engine" if resource_only => {
            let target = install_dir.join("bin").join(ytdlp_binary_name());
            copy_required_file(
                &ytdlp_source_path()?,
                &target,
                "未找到随应用分发的 yt-dlp。",
            )?;
            make_executable(&target)?;
        }
        "media-engine" if resource_only => {
            let target = install_dir.join("bin").join(ffmpeg_binary_name());
            copy_required_file(
                &ffmpeg_source_path()?,
                &target,
                "未找到随应用分发的 FFmpeg。",
            )?;
            make_executable(&target)?;
        }
        _ => {}
    }

    Ok(())
}

fn copy_optional_file(source: &Path, target: &Path) -> Result<(), String> {
    if !source.is_file() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建 pack 资源目录失败：{error}"))?;
    }
    fs::copy(source, target).map_err(|error| format!("复制 pack 资源失败：{error}"))?;
    Ok(())
}

fn copy_required_file(source: &Path, target: &Path, message: &str) -> Result<(), String> {
    if !source.is_file() {
        return Err(message.to_string());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建 pack 资源目录失败：{error}"))?;
    }
    fs::copy(source, target).map_err(|error| format!("复制 pack 资源失败：{error}"))?;
    Ok(())
}

fn copy_optional_dir(source: &Path, target: &Path) -> Result<(), String> {
    if !source.is_dir() {
        return Ok(());
    }
    if target.exists() {
        fs::remove_dir_all(target).map_err(|error| format!("清理旧资源目录失败：{error}"))?;
    }
    copy_dir_all(source, target)
}

fn copy_dir_all(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|error| format!("创建 pack 资源目录失败：{error}"))?;
    for entry in fs::read_dir(source).map_err(|error| format!("读取资源目录失败：{error}"))?
    {
        let entry = entry.map_err(|error| format!("读取资源目录条目失败：{error}"))?;
        let path = entry.path();
        let destination = target.join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &destination)?;
        } else {
            fs::copy(&path, &destination)
                .map_err(|error| format!("复制 pack 资源失败：{error}"))?;
        }
    }
    Ok(())
}

fn cleanup_artifact(artifact: InstallArtifact) {
    if let InstallArtifact::Temporary(path, _) = artifact {
        let _ = fs::remove_file(path);
    }
}

fn download_install_artifact(url: &str, binary_name: &str) -> Result<InstallArtifact, String> {
    if let Some(path) = url.strip_prefix("file://") {
        let path = PathBuf::from(path);
        return Ok(InstallArtifact::Existing(
            path.clone(),
            artifact_kind_for_path(&path, binary_name),
        ));
    }

    let kind = artifact_kind_for_url(url, binary_name);
    let temp_path = env::temp_dir().join(format!(
        "{}-{}.{}",
        binary_name,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_millis())
            .unwrap_or(0),
        if matches!(kind, InstallArtifactKind::ZipBundle) {
            "zip"
        } else {
            "download"
        }
    ));

    if curl_available() {
        if download_with_curl(url, &temp_path).is_ok() {
            return Ok(InstallArtifact::Temporary(temp_path, kind));
        }
    }

    let client = Client::builder()
        .http1_only()
        .build()
        .map_err(|error| format!("初始化下载客户端失败：{error}"))?;
    let response = client
        .get(url)
        .header("User-Agent", "StreamVerse/0.1.0")
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .map_err(|error| format!("下载 pack 失败：{error}"))?;

    if !response.status().is_success() {
        return Err(format!("下载 pack 失败：HTTP {}", response.status()));
    }

    let mut file =
        fs::File::create(&temp_path).map_err(|error| format!("创建临时 pack 文件失败：{error}"))?;
    let mut response = response;
    match response.copy_to(&mut file) {
        Ok(_) => {}
        Err(error) => {
            if download_with_curl(url, &temp_path).is_err() {
                return Err(format!("读取 pack 内容失败：{error}"));
            }
        }
    }

    Ok(InstallArtifact::Temporary(temp_path, kind))
}

fn read_installed_manifest(pack_id: &str) -> Option<InstalledPackManifest> {
    let path = installed_manifest_path(pack_id);
    let raw = fs::read(&path).ok()?;
    serde_json::from_slice::<InstalledPackManifest>(&raw).ok()
}

fn installed_manifest_path(pack_id: &str) -> PathBuf {
    installed_pack_dir(pack_id).join("manifest.json")
}

fn registry_pack_size(registry_pack: &RegistryPack) -> Option<u64> {
    match &registry_pack.source {
        RegistryPackSource::LocalBuild => {
            locate_pack_source_binary(&registry_pack.binary_name).and_then(file_size)
        }
        RegistryPackSource::Url { url } => registry_pack.size_bytes.or_else(|| {
            url.strip_prefix("file://")
                .map(PathBuf::from)
                .and_then(file_size)
        }),
    }
}

fn file_size(path: PathBuf) -> Option<u64> {
    path.metadata().ok().map(|metadata| metadata.len())
}

fn can_execute_ytdlp(path: &Path) -> bool {
    std::process::Command::new(path)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn ffmpeg_source_path() -> Result<PathBuf, String> {
    if let Some(resources) = packaged_resources_root() {
        let bundled = resources.join("bin").join(ffmpeg_binary_name());
        if bundled.is_file() {
            return Ok(bundled);
        }
    }

    let path = workspace_root()
        .join("node_modules")
        .join("ffmpeg-static")
        .join(ffmpeg_binary_name());
    if path.is_file() {
        Ok(path)
    } else {
        Err("未找到本地 FFmpeg 二进制，请先执行 npm install。".to_string())
    }
}

fn ytdlp_source_path() -> Result<PathBuf, String> {
    if let Some(root) = packaged_pack_resource_root("download-engine") {
        let bundled = root.join("bin").join(ytdlp_binary_name());
        if bundled.is_file() {
            return Ok(bundled);
        }
    }

    let path = workspace_root()
        .join("src-tauri")
        .join("gen")
        .join("resources")
        .join("download-engine")
        .join("bin")
        .join(ytdlp_binary_name());
    if path.is_file() {
        Ok(path)
    } else {
        Err("未找到本地 yt-dlp 二进制，请先准备打包资源。".to_string())
    }
}

fn ffmpeg_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

fn ytdlp_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

fn estimated_ytdlp_size() -> u64 {
    if cfg!(target_os = "windows") {
        18 * 1024 * 1024
    } else {
        6 * 1024 * 1024
    }
}

fn curl_available() -> bool {
    std::process::Command::new("curl")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn artifact_kind_for_path(path: &Path, binary_name: &str) -> InstallArtifactKind {
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("zip"))
    {
        InstallArtifactKind::ZipBundle
    } else if path
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.ends_with(&binary_filename(binary_name)))
    {
        InstallArtifactKind::Binary
    } else {
        InstallArtifactKind::Binary
    }
}

fn artifact_kind_for_url(url: &str, binary_name: &str) -> InstallArtifactKind {
    let path = url.split('?').next().unwrap_or(url);
    artifact_kind_for_path(Path::new(path), binary_name)
}

fn verify_sha256(path: &Path, expected_sha256: &str) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|error| format!("读取 pack 文件失败：{error}"))?;
    let digest = Sha256::digest(&bytes);
    let actual = format!("{digest:x}");
    let expected = expected_sha256.trim().to_lowercase();

    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "pack 校验失败：期望 sha256 为 {expected}，实际为 {actual}。"
        ))
    }
}

fn locate_pack_source_binary(binary_name: &str) -> Option<PathBuf> {
    let binary_file = binary_filename(binary_name);
    let mut candidates = Vec::new();

    if let Ok(current_exe) = env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            candidates.push(parent.join(&binary_file));
            candidates.push(parent.join("binaries").join(&binary_file));
            if let Some(resources) = packaged_resources_root() {
                candidates.push(resources.join("pack-binaries").join(&binary_file));
            }
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    candidates.push(manifest_dir.join("target").join("debug").join(&binary_file));
    candidates.push(
        manifest_dir
            .join("target")
            .join("release")
            .join(&binary_file),
    );
    candidates.push(manifest_dir.join("binaries").join(&binary_file));

    candidates.into_iter().find(|path| path.is_file())
}

fn default_registry_pack(pack_id: &str, binary_name: &str) -> RegistryPack {
    RegistryPack {
        id: pack_id.to_string(),
        binary_name: binary_name.to_string(),
        version: Some(env!("CARGO_PKG_VERSION").to_string()),
        resource_only: false,
        target: Some(current_target().to_string()),
        size_bytes: None,
        sha256: None,
        source: RegistryPackSource::LocalBuild,
    }
}

fn default_shared_registry_pack(pack_id: &str) -> RegistryPack {
    match pack_id {
        "download-engine" => RegistryPack {
            id: pack_id.to_string(),
            binary_name: ytdlp_binary_name().to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            resource_only: true,
            target: Some(current_target().to_string()),
            size_bytes: Some(estimated_ytdlp_size()),
            sha256: None,
            source: RegistryPackSource::LocalBuild,
        },
        _ => RegistryPack {
            id: pack_id.to_string(),
            binary_name: pack_id.to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            resource_only: true,
            target: Some(current_target().to_string()),
            size_bytes: None,
            sha256: None,
            source: RegistryPackSource::LocalBuild,
        },
    }
}

impl RegistryPackSource {
    fn kind_label(&self) -> &'static str {
        match self {
            RegistryPackSource::LocalBuild => "localBuild",
            RegistryPackSource::Url { .. } => "url",
        }
    }
}

fn registry_path() -> PathBuf {
    if let Ok(path) = env::var(REGISTRY_PATH_ENV) {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }

    if let Some(path) = packaged_registry_path().filter(|path| path.is_file()) {
        return path;
    }

    workspace_root().join("registry").join("plugins.json")
}

fn packaged_resources_root() -> Option<PathBuf> {
    let current_exe = env::current_exe().ok()?;
    let parent = current_exe.parent()?;
    if let Some(contents_dir) = parent.parent() {
        let resources = contents_dir.join("Resources");
        if resources.is_dir() {
            return Some(resources);
        }
    }
    let resources = parent.join("resources");
    resources.is_dir().then_some(resources)
}

fn packaged_registry_path() -> Option<PathBuf> {
    let current_exe = env::current_exe().ok()?;
    let parent = current_exe.parent()?;
    let mut candidates = vec![
        parent.join("registry").join("plugins.json"),
        parent
            .join("resources")
            .join("registry")
            .join("plugins.json"),
    ];
    if let Some(resources_dir) = packaged_resources_root() {
        candidates.push(resources_dir.join("registry").join("plugins.json"));
    } else if let Some(contents_dir) = parent.parent() {
        candidates.push(
            contents_dir
                .join("Resources")
                .join("registry")
                .join("plugins.json"),
        );
    }

    candidates.into_iter().find(|path| path.is_file())
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
        .to_path_buf()
}

fn packaged_pack_resource_root(pack_id: &str) -> Option<PathBuf> {
    packaged_resources_root()
        .map(|root| root.join("pack-resources").join(pack_id))
        .filter(|path| path.exists())
}

fn shared_pack_size(pack_id: &str) -> Option<u64> {
    let shared = pack_registry::shared_pack_for_id(pack_id)?;
    let registry_pack =
        load_registry_pack(shared.id).unwrap_or_else(|| default_shared_registry_pack(shared.id));

    registry_pack
        .size_bytes
        .or_else(|| registry_pack_size(&registry_pack))
        .or_else(|| resolve_shared_pack_size_from_disk(shared.id))
}

fn resolve_shared_pack_size_from_disk(pack_id: &str) -> Option<u64> {
    let install_dir = installed_pack_dir(pack_id);
    dir_size(&install_dir).or_else(|| {
        packaged_pack_resource_root(pack_id)
            .and_then(|path| dir_size(&path))
            .or_else(|| {
                packaged_resources_root().and_then(|root| match pack_id {
                    "media-engine" => file_size(root.join("bin").join(ffmpeg_binary_name())),
                    _ => None,
                })
            })
    })
}

fn dir_size(path: &Path) -> Option<u64> {
    if !path.exists() {
        return None;
    }

    if path.is_file() {
        return file_size(path.to_path_buf());
    }

    let mut total = 0u64;
    for entry in fs::read_dir(path).ok()? {
        let entry = entry.ok()?;
        let entry_path = entry.path();
        total = total.saturating_add(dir_size(&entry_path)?);
    }
    Some(total)
}

fn download_with_curl(url: &str, target_path: &Path) -> Result<(), String> {
    let status = std::process::Command::new("curl")
        .arg("-L")
        .arg("--fail")
        .arg("-A")
        .arg("StreamVerse/0.1.0")
        .arg("-o")
        .arg(target_path)
        .arg(url)
        .status()
        .map_err(|error| format!("调用 curl 下载失败：{error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("curl 下载失败。".to_string())
    }
}

fn installed_pack_dir(pack_id: &str) -> PathBuf {
    packs_root_dir().join(pack_id)
}

fn installed_binary_path(pack_id: &str, binary_name: &str) -> PathBuf {
    installed_pack_dir(pack_id)
        .join("bin")
        .join(binary_filename(binary_name))
}

fn packs_root_dir() -> PathBuf {
    if let Ok(root) = env::var(PACKS_DIR_ENV) {
        if !root.trim().is_empty() {
            return PathBuf::from(root);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let root = env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        return PathBuf::from(root).join("StreamVerse").join("packs");
    }

    #[cfg(not(target_os = "windows"))]
    {
        let root = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(root).join(".streamverse").join("packs")
    }
}

fn current_target() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "macos-aarch64"
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "macos-x86_64"
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "windows-x86_64"
    }

    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "windows-aarch64"
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux-x86_64"
    }

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "linux-aarch64"
    }
}

fn binary_filename(binary_name: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        format!("{binary_name}.exe")
    }

    #[cfg(not(target_os = "windows"))]
    {
        binary_name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        binary_filename, ensure_download_engine_installed, ensure_media_engine_installed,
        estimated_ytdlp_size, install_pack_for_module, installed_binary_path, load_registry_pack,
        module_runtime_info, verify_sha256, PACKS_DIR_ENV, REGISTRY_PATH_ENV, REGISTRY_URL_ENV,
    };
    use crate::settings;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn temp_fixture_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "streamverse-pack-test-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn builds_installed_binary_path() {
        let path = installed_binary_path("douyin-pack", "streamverse-pack-douyin");
        assert!(path.ends_with(
            Path::new("douyin-pack")
                .join("bin")
                .join(binary_filename("streamverse-pack-douyin"))
        ));
    }

    #[test]
    fn installs_pack_from_registry_file_url() {
        let _guard = env_lock().lock().unwrap();
        let fixture_dir = temp_fixture_dir("install");
        let packs_dir = fixture_dir.join("packs");
        let registry_path = fixture_dir.join("plugins.json");
        let source_binary = fixture_dir.join("streamverse-pack-youtube");
        let ytdlp_binary = fixture_dir.join("yt-dlp");
        let sha = "da9108707b1f98086aa3c4133b046c400ddaf130961e4c7a60f07b26663cc6bd";

        let _ = fs::remove_dir_all(&fixture_dir);
        fs::create_dir_all(&fixture_dir).unwrap();
        fs::write(&source_binary, b"youtube-pack-binary").unwrap();
        fs::write(&ytdlp_binary, b"yt-dlp-binary").unwrap();
        fs::write(
            &registry_path,
            format!(
                r#"{{
  "packs": [
    {{
      "id": "download-engine",
      "binaryName": "yt-dlp",
      "version": "latest",
      "target": "macos-aarch64",
      "source": {{
        "kind": "url",
        "url": "file://{}"
      }}
    }},
    {{
      "id": "youtube-pack",
      "binaryName": "streamverse-pack-youtube",
      "version": "0.2.0",
      "target": "macos-aarch64",
      "sha256": "{sha}",
      "source": {{
        "kind": "url",
        "url": "file://{}"
      }}
    }}
  ]
}}"#,
                ytdlp_binary.display(),
                source_binary.display()
            ),
        )
        .unwrap();

        std::env::set_var(REGISTRY_PATH_ENV, &registry_path);
        std::env::set_var(PACKS_DIR_ENV, &packs_dir);

        let mut app_settings = settings::AppSettings::default();
        install_pack_for_module("youtube-single", &mut app_settings).unwrap();

        let installed = packs_dir
            .join("youtube-pack")
            .join("bin")
            .join("streamverse-pack-youtube");
        let manifest = packs_dir.join("youtube-pack").join("manifest.json");
        assert!(installed.is_file());
        assert!(manifest.is_file());
        assert!(fs::read_to_string(&manifest)
            .unwrap()
            .contains("\"version\": \"0.2.0\""));
        assert!(app_settings.modules["youtube-single"].installed);
        assert!(app_settings.modules["youtube-single"].enabled);

        std::env::remove_var(REGISTRY_PATH_ENV);
        std::env::remove_var(PACKS_DIR_ENV);
        let _ = fs::remove_dir_all(&fixture_dir);
    }

    #[test]
    fn installs_browser_bridge_dependency_for_profile_module() {
        let _guard = env_lock().lock().unwrap();
        let fixture_dir = temp_fixture_dir("shared-dependency");
        let packs_dir = fixture_dir.join("packs");
        let registry_path = fixture_dir.join("plugins.json");
        let source_binary = fixture_dir.join("streamverse-pack-douyin");
        let ytdlp_binary = fixture_dir.join("yt-dlp");
        let _ = fs::remove_dir_all(&fixture_dir);
        fs::create_dir_all(&fixture_dir).unwrap();
        fs::write(&source_binary, b"douyin-pack-binary").unwrap();
        fs::write(&ytdlp_binary, b"yt-dlp-binary").unwrap();
        let sha = format!("{:x}", Sha256::digest(fs::read(&source_binary).unwrap()));
        fs::write(
            &registry_path,
            format!(
                r#"{{
  "packs": [
    {{
      "id": "download-engine",
      "binaryName": "yt-dlp",
      "version": "latest",
      "target": "macos-aarch64",
      "source": {{
        "kind": "url",
        "url": "file://{}"
      }}
    }},
    {{
      "id": "browser-bridge",
      "binaryName": "browser-bridge",
      "version": "0.1.0",
      "resourceOnly": true,
      "target": "macos-aarch64",
      "source": {{
        "kind": "localBuild"
      }}
    }},
    {{
      "id": "douyin-pack",
      "binaryName": "streamverse-pack-douyin",
      "version": "0.1.0",
      "target": "macos-aarch64",
      "sha256": "{sha}",
      "source": {{
        "kind": "url",
        "url": "file://{}"
      }}
    }}
  ]
}}"#,
                ytdlp_binary.display(),
                source_binary.display()
            ),
        )
        .unwrap();

        std::env::set_var(REGISTRY_PATH_ENV, &registry_path);
        std::env::set_var(PACKS_DIR_ENV, &packs_dir);

        let mut app_settings = settings::AppSettings::default();
        install_pack_for_module("douyin-profile", &mut app_settings).unwrap();

        assert!(packs_dir
            .join("browser-bridge")
            .join("scripts")
            .join("profile_browser_scan.py")
            .is_file());

        std::env::remove_var(REGISTRY_PATH_ENV);
        std::env::remove_var(PACKS_DIR_ENV);
        let _ = fs::remove_dir_all(&fixture_dir);
    }

    #[test]
    fn installs_media_engine_shared_pack_on_demand() {
        let _guard = env_lock().lock().unwrap();
        let fixture_dir = temp_fixture_dir("media-engine");
        let packs_dir = fixture_dir.join("packs");

        let _ = fs::remove_dir_all(&fixture_dir);
        fs::create_dir_all(&fixture_dir).unwrap();

        std::env::set_var(PACKS_DIR_ENV, &packs_dir);
        std::env::remove_var(REGISTRY_PATH_ENV);
        std::env::remove_var(REGISTRY_URL_ENV);

        let ffmpeg_path = ensure_media_engine_installed()
            .unwrap()
            .expect("ffmpeg path should be installed");
        assert!(ffmpeg_path.is_file());
        assert_eq!(
            ffmpeg_path.file_name().and_then(|value| value.to_str()),
            Some(if cfg!(target_os = "windows") {
                "ffmpeg.exe"
            } else {
                "ffmpeg"
            })
        );

        std::env::remove_var(PACKS_DIR_ENV);
        let _ = fs::remove_dir_all(&fixture_dir);
    }

    #[test]
    fn installs_download_engine_shared_pack_on_demand() {
        let _guard = env_lock().lock().unwrap();
        let fixture_dir = temp_fixture_dir("download-engine");
        let packs_dir = fixture_dir.join("packs");

        let _ = fs::remove_dir_all(&fixture_dir);
        fs::create_dir_all(&fixture_dir).unwrap();

        std::env::set_var(PACKS_DIR_ENV, &packs_dir);
        std::env::remove_var(REGISTRY_PATH_ENV);
        std::env::remove_var(REGISTRY_URL_ENV);

        let ytdlp_path = ensure_download_engine_installed()
            .unwrap()
            .expect("yt-dlp path should be installed");
        assert!(ytdlp_path.is_file());
        assert_eq!(
            ytdlp_path.file_name().and_then(|value| value.to_str()),
            Some(if cfg!(target_os = "windows") {
                "yt-dlp.exe"
            } else {
                "yt-dlp"
            })
        );

        std::env::remove_var(PACKS_DIR_ENV);
        let _ = fs::remove_dir_all(&fixture_dir);
    }

    #[test]
    fn installs_pack_from_zip_bundle() {
        let _guard = env_lock().lock().unwrap();
        let fixture_dir = temp_fixture_dir("zip-install");
        let packs_dir = fixture_dir.join("packs");
        let registry_path = fixture_dir.join("plugins.json");
        let zip_path = fixture_dir.join("youtube-pack.zip");
        let ytdlp_binary = fixture_dir.join("yt-dlp");
        let _ = fs::remove_dir_all(&fixture_dir);
        fs::create_dir_all(&fixture_dir).unwrap();
        fs::write(&ytdlp_binary, b"yt-dlp-binary").unwrap();
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut archive = ZipWriter::new(file);
            let options = SimpleFileOptions::default();
            archive
                .start_file("bin/streamverse-pack-youtube", options)
                .unwrap();
            archive.write_all(b"youtube-pack-binary").unwrap();
            archive.finish().unwrap();
        }
        let sha = format!("{:x}", Sha256::digest(fs::read(&zip_path).unwrap()));
        fs::write(
            &registry_path,
            format!(
                r#"{{
  "packs": [
    {{
      "id": "download-engine",
      "binaryName": "yt-dlp",
      "version": "latest",
      "target": "macos-aarch64",
      "source": {{
        "kind": "url",
        "url": "file://{}"
      }}
    }},
    {{
      "id": "youtube-pack",
      "binaryName": "streamverse-pack-youtube",
      "version": "0.3.0",
      "target": "macos-aarch64",
      "sha256": "{sha}",
      "source": {{
        "kind": "url",
        "url": "file://{}"
      }}
    }}
  ]
}}"#,
                ytdlp_binary.display(),
                zip_path.display()
            ),
        )
        .unwrap();

        std::env::set_var(REGISTRY_PATH_ENV, &registry_path);
        std::env::set_var(PACKS_DIR_ENV, &packs_dir);

        let mut app_settings = settings::AppSettings::default();
        install_pack_for_module("youtube-single", &mut app_settings).unwrap();

        let installed = packs_dir
            .join("youtube-pack")
            .join("bin")
            .join("streamverse-pack-youtube");
        assert!(installed.is_file());
        assert_eq!(fs::read(&installed).unwrap(), b"youtube-pack-binary");

        std::env::remove_var(REGISTRY_PATH_ENV);
        std::env::remove_var(PACKS_DIR_ENV);
        let _ = fs::remove_dir_all(&fixture_dir);
    }

    #[test]
    fn reads_registry_entry_for_current_target() {
        let _guard = env_lock().lock().unwrap();
        let fixture_dir = temp_fixture_dir("registry");
        let registry_path = fixture_dir.join("plugins.json");

        let _ = fs::remove_dir_all(&fixture_dir);
        fs::create_dir_all(&fixture_dir).unwrap();
        fs::write(
            &registry_path,
            r#"{
  "packs": [
    {
      "id": "douyin-pack",
      "binaryName": "streamverse-pack-douyin",
      "target": "macos-aarch64",
      "sizeBytes": 1024,
      "source": { "kind": "localBuild" }
    }
  ]
}"#,
        )
        .unwrap();

        std::env::set_var(REGISTRY_PATH_ENV, &registry_path);
        let pack = load_registry_pack("douyin-pack").unwrap();
        assert_eq!(pack.size_bytes, Some(1024));
        std::env::remove_var(REGISTRY_PATH_ENV);
        let _ = fs::remove_dir_all(&fixture_dir);
    }

    #[test]
    fn reads_registry_entry_from_registry_url() {
        let _guard = env_lock().lock().unwrap();
        let fixture_dir = temp_fixture_dir("registry-url");
        let registry_path = fixture_dir.join("plugins.json");

        let _ = fs::remove_dir_all(&fixture_dir);
        fs::create_dir_all(&fixture_dir).unwrap();
        fs::write(
            &registry_path,
            r#"{
  "packs": [
    {
      "id": "youtube-pack",
      "binaryName": "streamverse-pack-youtube",
      "target": "macos-aarch64",
      "source": { "kind": "localBuild" }
    }
  ]
}"#,
        )
        .unwrap();

        std::env::set_var(
            REGISTRY_URL_ENV,
            format!("file://{}", registry_path.display()),
        );
        let pack = load_registry_pack("youtube-pack").unwrap();
        assert_eq!(pack.binary_name, "streamverse-pack-youtube");
        std::env::remove_var(REGISTRY_URL_ENV);
        let _ = fs::remove_dir_all(&fixture_dir);
    }

    #[test]
    fn reports_update_available_when_registry_version_is_newer() {
        let _guard = env_lock().lock().unwrap();
        let fixture_dir = temp_fixture_dir("update");
        let packs_dir = fixture_dir.join("packs");
        let registry_path = fixture_dir.join("plugins.json");
        let installed_dir = packs_dir.join("youtube-pack");
        let installed_bin = installed_dir.join("bin").join("streamverse-pack-youtube");
        let manifest = installed_dir.join("manifest.json");

        let _ = fs::remove_dir_all(&fixture_dir);
        fs::create_dir_all(installed_bin.parent().unwrap()).unwrap();
        fs::write(&installed_bin, b"youtube-pack-binary").unwrap();
        fs::write(
            &manifest,
            r#"{
  "id": "youtube-pack",
  "binaryName": "streamverse-pack-youtube",
  "version": "0.1.0",
  "sourceKind": "url",
  "installedAt": 1
}"#,
        )
        .unwrap();
        fs::write(
            &registry_path,
            r#"{
  "packs": [
    {
      "id": "youtube-pack",
      "binaryName": "streamverse-pack-youtube",
      "version": "0.2.0",
      "target": "macos-aarch64",
      "sizeBytes": 2048,
      "source": {
        "kind": "localBuild"
      }
    }
  ]
}"#,
        )
        .unwrap();

        std::env::set_var(REGISTRY_PATH_ENV, &registry_path);
        std::env::set_var(PACKS_DIR_ENV, &packs_dir);

        let info = module_runtime_info("youtube-single").expect("runtime info");
        assert_eq!(info.pack_id, "youtube-pack");
        assert_eq!(info.current_version.as_deref(), Some("0.1.0"));
        assert_eq!(info.latest_version.as_deref(), Some("0.2.0"));
        assert!(info.update_available);
        assert!(
            info.size_bytes
                .is_some_and(|value| value >= 2048 + estimated_ytdlp_size()),
            "unexpected runtime size: {:?}",
            info.size_bytes
        );

        std::env::remove_var(REGISTRY_PATH_ENV);
        std::env::remove_var(PACKS_DIR_ENV);
        let _ = fs::remove_dir_all(&fixture_dir);
    }

    #[test]
    fn sha256_verification_rejects_mismatch() {
        let fixture_dir = temp_fixture_dir("sha");
        let sample = fixture_dir.join("sample.bin");

        let _ = fs::remove_dir_all(&fixture_dir);
        fs::create_dir_all(&fixture_dir).unwrap();
        fs::write(&sample, b"mismatch").unwrap();

        let error = verify_sha256(&sample, "deadbeef").unwrap_err();
        assert!(error.contains("pack 校验失败"));

        let _ = fs::remove_dir_all(&fixture_dir);
    }
}
