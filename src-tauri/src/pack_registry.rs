#[derive(Clone, Copy)]
pub struct LocalPack {
    pub id: &'static str,
    pub binary_name: &'static str,
    pub supports_single: bool,
    pub supports_profile: bool,
    pub module_ids: &'static [&'static str],
}

#[derive(Clone, Copy)]
pub struct SharedPack {
    pub id: &'static str,
}

const DOUYIN_MODULES: [&str; 2] = ["douyin-single", "douyin-profile"];
const BILIBILI_MODULES: [&str; 2] = ["bilibili-single", "bilibili-profile"];
const YOUTUBE_MODULES: [&str; 1] = ["youtube-single"];
const DOUYIN_SINGLE_DEPS: [&str; 1] = ["download-engine"];
const DOUYIN_PROFILE_DEPS: [&str; 2] = ["browser-bridge", "download-engine"];
const BILIBILI_SINGLE_DEPS: [&str; 2] = ["download-engine", "media-engine"];
const BILIBILI_PROFILE_DEPS: [&str; 3] = ["browser-bridge", "download-engine", "media-engine"];
const YOUTUBE_SINGLE_DEPS: [&str; 2] = ["download-engine", "media-engine"];
const MEDIA_ENGINE_PACK: SharedPack = SharedPack { id: "media-engine" };
const BROWSER_BRIDGE_PACK: SharedPack = SharedPack {
    id: "browser-bridge",
};
const DOWNLOAD_ENGINE_PACK: SharedPack = SharedPack {
    id: "download-engine",
};

const DOUYIN_PACK: LocalPack = LocalPack {
    id: "douyin-pack",
    binary_name: "streamverse-pack-douyin",
    supports_single: true,
    supports_profile: true,
    module_ids: &DOUYIN_MODULES,
};

const BILIBILI_PACK: LocalPack = LocalPack {
    id: "bilibili-pack",
    binary_name: "streamverse-pack-bilibili",
    supports_single: true,
    supports_profile: true,
    module_ids: &BILIBILI_MODULES,
};

const YOUTUBE_PACK: LocalPack = LocalPack {
    id: "youtube-pack",
    binary_name: "streamverse-pack-youtube",
    supports_single: true,
    supports_profile: false,
    module_ids: &YOUTUBE_MODULES,
};

pub fn local_pack_for_platform(platform: &str) -> Option<LocalPack> {
    match platform {
        "douyin" => Some(DOUYIN_PACK),
        "bilibili" => Some(BILIBILI_PACK),
        "youtube" => Some(YOUTUBE_PACK),
        _ => None,
    }
}

pub fn local_pack_for_module(module_id: &str) -> Option<LocalPack> {
    [DOUYIN_PACK, BILIBILI_PACK, YOUTUBE_PACK]
        .into_iter()
        .find(|pack| pack.module_ids.contains(&module_id))
}

pub fn local_pack_for_binary(binary_name: &str) -> Option<LocalPack> {
    [DOUYIN_PACK, BILIBILI_PACK, YOUTUBE_PACK]
        .into_iter()
        .find(|pack| pack.binary_name == binary_name)
}

pub fn shared_pack_for_id(pack_id: &str) -> Option<SharedPack> {
    [MEDIA_ENGINE_PACK, BROWSER_BRIDGE_PACK, DOWNLOAD_ENGINE_PACK]
        .into_iter()
        .find(|pack| pack.id == pack_id)
}

pub fn shared_dependencies_for_module(module_id: &str) -> &'static [&'static str] {
    match module_id {
        "douyin-single" => &DOUYIN_SINGLE_DEPS,
        "douyin-profile" => &DOUYIN_PROFILE_DEPS,
        "bilibili-single" => &BILIBILI_SINGLE_DEPS,
        "bilibili-profile" => &BILIBILI_PROFILE_DEPS,
        "youtube-single" => &YOUTUBE_SINGLE_DEPS,
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::{local_pack_for_binary, local_pack_for_module, shared_dependencies_for_module};

    #[test]
    fn resolves_shared_pack_for_module() {
        let pack = local_pack_for_module("douyin-profile").unwrap();
        assert_eq!(pack.id, "douyin-pack");
        assert_eq!(pack.binary_name, "streamverse-pack-douyin");
    }

    #[test]
    fn resolves_pack_from_binary_name() {
        let pack = local_pack_for_binary("streamverse-pack-bilibili").unwrap();
        assert_eq!(pack.id, "bilibili-pack");
    }

    #[test]
    fn resolves_module_shared_dependencies() {
        assert_eq!(
            shared_dependencies_for_module("douyin-single"),
            &["download-engine"]
        );
        assert_eq!(
            shared_dependencies_for_module("douyin-profile"),
            &["browser-bridge", "download-engine"]
        );
        assert_eq!(
            shared_dependencies_for_module("bilibili-profile"),
            &["browser-bridge", "download-engine", "media-engine"]
        );
        assert_eq!(
            shared_dependencies_for_module("youtube-single"),
            &["download-engine", "media-engine"]
        );
    }
}
