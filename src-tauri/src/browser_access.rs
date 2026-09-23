//! User-selected browser directory access. Restoring access never shows a prompt.
#[cfg(target_os = "macos")]
mod macos {
    use objc2::rc::Retained;
    use objc2_foundation::{
        NSData, NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions, NSURL,
    };
    use std::{fs, path::PathBuf};

    pub struct AccessGuard(Retained<NSURL>);
    impl Drop for AccessGuard {
        fn drop(&mut self) {
            // SAFETY: paired with a successful start on this retained URL.
            unsafe { self.0.stopAccessingSecurityScopedResource() };
        }
    }

    fn browser_root(browser: &str) -> Result<PathBuf, String> {
        let relative = match browser {
            "chrome" => "Library/Application Support/Google/Chrome",
            "edge" => "Library/Application Support/Microsoft Edge",
            _ => return Err("此浏览器不支持目录授权。".into()),
        };
        Ok(PathBuf::from(crate::settings::home_dir()).join(relative))
    }

    fn bookmark_path(browser: &str) -> Result<PathBuf, String> {
        browser_root(browser)?; // validate before using browser in a filename
        Ok(crate::settings::app_data_root().join(format!("browser-access-{browser}.json")))
    }

    fn save_bookmark(browser: &str, url: &NSURL) -> Result<(), String> {
        let data = url
            .bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
                NSURLBookmarkCreationOptions::WithSecurityScope
                    | NSURLBookmarkCreationOptions::SecurityScopeAllowOnlyReadAccess,
                None,
                None,
            )
            .map_err(|_| "无法保存目录授权，请重新选择目录。".to_string())?;
        crate::persistence::save_json(&bookmark_path(browser)?, &data.to_vec())
    }

    pub fn restore(browser: &str) -> Option<AccessGuard> {
        let bytes: Vec<u8> =
            serde_json::from_slice(&fs::read(bookmark_path(browser).ok()?).ok()?).ok()?;
        let data = NSData::with_bytes(&bytes);
        let mut stale = objc2::runtime::Bool::NO;
        // SAFETY: stale points to a live Bool; the result retains the resolved URL.
        let url = unsafe {
            NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
                &data,
                NSURLBookmarkResolutionOptions::WithSecurityScope
                    | NSURLBookmarkResolutionOptions::WithoutUI,
                None,
                &mut stale,
            )
            .ok()?
        };
        if url.to_file_path()? != browser_root(browser).ok()? {
            return None;
        }
        // SAFETY: access remains active until AccessGuard is dropped.
        if !unsafe { url.startAccessingSecurityScopedResource() } {
            return None;
        }
        let guard = AccessGuard(url);
        if stale.as_bool() {
            let _ = save_bookmark(browser, &guard.0);
        }
        Some(guard)
    }

    pub fn authorize(browser: &str) -> Result<(), String> {
        let expected = browser_root(browser)?;
        let selected = rfd::FileDialog::new()
            .set_title("允许 StreamVerse 访问所选浏览器目录（无需完全磁盘访问）")
            .set_directory(&expected)
            .pick_folder()
            .ok_or_else(|| "已取消目录授权，现有登录态未更改。".to_string())?;
        if selected != expected {
            return Err("请选择窗口预先定位的浏览器数据目录。".into());
        }
        fs::read_dir(&selected).map_err(|_| "所选目录仍不可读取，授权尚未生效。".to_string())?;
        let url =
            NSURL::from_file_path(&selected).ok_or_else(|| "无法识别所选目录。".to_string())?;
        save_bookmark(browser, &url)?;
        let _access = restore(browser)
            .ok_or_else(|| "目录授权未能恢复，请重新授权浏览器目录。".to_string())?;
        fs::read_dir(&selected).map_err(|_| "目录授权尚未生效。".to_string())?;
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn only_known_browser_roots_are_authorizable() {
            assert!(super::browser_root("chrome")
                .unwrap()
                .ends_with("Google/Chrome"));
            for id in ["../settings", "safari", "", "Chrome"] {
                assert!(super::bookmark_path(id).is_err());
                assert!(super::restore(id).is_none());
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::{authorize, restore};

#[cfg(not(target_os = "macos"))]
pub fn restore(_browser: &str) -> Option<()> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn authorize(_browser: &str) -> Result<(), String> {
    Err("当前系统不需要 macOS 浏览器目录授权。".into())
}
