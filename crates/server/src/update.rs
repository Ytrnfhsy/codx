//! Self-update from GitHub Releases, for both the game and the dedicated server.
//!
//! CI publishes every `v*` tag as a release whose assets include the bare binaries,
//! named `<bin>-<platform>[.exe]` (`toon-warfare-windows-x64.exe`, `toon-server-linux-x64`,
//! …). A release build asks GitHub for the latest release; if it is newer, it downloads
//! its own binary next to itself and swaps it in (`self-replace` also handles Windows,
//! where a running program cannot be overwritten), then restarts.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use toon_shared::consts::GAME_VERSION;

/// GitHub repository the releases come from (CI sets `TW_UPDATE_REPO` to its own).
pub const REPO: &str = match option_env!("TW_UPDATE_REPO") {
    Some(r) => r,
    None => "ytrnfhsy/codx",
};

/// Release asset suffix for this system, or None when no build is published for it.
pub fn platform() -> Option<&'static str> {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("windows-x64")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux-x64")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("macos-arm64")
    } else {
        None
    }
}

/// File name of a binary in a release, for this system.
pub fn asset_name(bin: &str) -> Option<String> {
    let ext = if cfg!(windows) { ".exe" } else { "" };
    Some(format!("{bin}-{}{ext}", platform()?))
}

#[derive(Clone, Debug)]
pub struct Release {
    /// Without the leading `v`.
    pub version: String,
    pub url: String,
    pub size: u64,
}

/// True if version `a` is newer than `b` ("0.10.0" > "0.9.3"; anything after a `-` is
/// ignored).
pub fn newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.trim_start_matches('v')
            .split('-')
            .next()
            .unwrap_or("")
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    let (a, b) = (parts(a), parts(b));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x > y;
        }
    }
    false
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(30))
        .try_proxy_from_env(true)
        .user_agent(&format!("toon-warfare/{GAME_VERSION}"))
        .build()
}

/// The latest release if it is newer than this build and has `bin` for this system.
pub fn check(bin: &str) -> Result<Option<Release>, String> {
    // TOON_UPDATE_URL points somewhere else with the same JSON (a mirror, or a test)
    let url = std::env::var("TOON_UPDATE_URL")
        .unwrap_or_else(|_| format!("https://api.github.com/repos/{REPO}/releases/latest"));
    let body = agent()
        .get(&url)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(404, _) => "релізів ще немає".to_string(),
            ureq::Error::Status(c, _) => format!("GitHub відповів {c}"),
            ureq::Error::Transport(_) => "немає зв'язку з GitHub".to_string(),
        })?
        .into_string()
        .map_err(|e| e.to_string())?;
    parse_latest(&body, bin)
}

/// Picks our asset out of GitHub's "latest release" JSON.
pub fn parse_latest(body: &str, bin: &str) -> Result<Option<Release>, String> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let tag = v["tag_name"].as_str().ok_or("у релізі немає тега")?;
    let version = tag.trim_start_matches('v').to_string();
    if !newer(&version, GAME_VERSION) {
        return Ok(None);
    }
    let want = asset_name(bin).ok_or("для твоєї системи збірок немає")?;
    let asset = v["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|a| a["name"].as_str() == Some(want.as_str())))
        .ok_or_else(|| format!("у релізі v{version} немає {want}"))?;
    Ok(Some(Release {
        version,
        url: asset["browser_download_url"]
            .as_str()
            .ok_or("немає посилання на файл")?
            .to_string(),
        size: asset["size"].as_u64().unwrap_or(0),
    }))
}

/// Where the program file was before it got replaced: on Linux `current_exe()` then
/// reads "… (deleted)", which cannot be started again.
static EXE: OnceLock<PathBuf> = OnceLock::new();

/// Downloads the release and puts it in place of the running program. `progress` gets
/// 0..1. The new version starts with the next launch (see `restart`).
pub fn install(rel: &Release, mut progress: impl FnMut(f32)) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = EXE.get_or_init(|| exe).clone();
    let dir = exe.parent().ok_or("невідома тека програми")?;
    let tmp: PathBuf = dir.join(format!(
        ".{}.update",
        exe.file_name().and_then(|n| n.to_str()).unwrap_or("toon")
    ));
    let res = (|| {
        let resp = agent().get(&rel.url).call().map_err(|e| e.to_string())?;
        let mut file = std::fs::File::create(&tmp)
            .map_err(|e| format!("немає права писати в {} ({e})", dir.display()))?;
        let mut reader = resp.into_reader();
        let mut buf = vec![0u8; 64 * 1024];
        let mut got = 0u64;
        loop {
            let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            got += n as u64;
            if rel.size > 0 {
                progress((got as f32 / rel.size as f32).min(1.0));
            }
        }
        file.flush().map_err(|e| e.to_string())?;
        drop(file);
        if rel.size > 0 && got != rel.size {
            return Err(format!("завантажено {got} з {} байт", rel.size));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
        self_replace::self_replace(&tmp).map_err(|e| format!("не вдалося замінити файл: {e}"))
    })();
    let _ = std::fs::remove_file(&tmp);
    res
}

/// Starts the (updated) program again with the same arguments and exits this one.
pub fn restart() -> ! {
    let exe = EXE.get().cloned().or_else(|| {
        let e = std::env::current_exe().ok()?;
        let s = e.to_string_lossy();
        Some(PathBuf::from(
            s.strip_suffix(" (deleted)").unwrap_or(&s).to_string(),
        ))
    });
    if let Some(exe) = exe {
        if let Err(e) = std::process::Command::new(&exe)
            .args(std::env::args_os().skip(1))
            .spawn()
        {
            eprintln!("не вдалося перезапустити {}: {e}", exe.display());
        }
    }
    std::process::exit(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert!(newer("0.10.0", "0.9.3"));
        assert!(newer("v1.0.0", "0.99.0"));
        assert!(newer("0.2", "0.1.9"));
        assert!(!newer("0.2.0", "0.2.0"));
        assert!(!newer("0.1.0", "0.2.0"));
        assert!(!newer("0.2.0-beta", "0.2.0"));
    }

    #[test]
    fn finds_our_binary_in_the_release() {
        let Some(name) = asset_name("toon-warfare") else {
            return;
        };
        let body = format!(
            r#"{{"tag_name":"v999.0.0","assets":[
                {{"name":"toon-server-x","browser_download_url":"https://x/s","size":1}},
                {{"name":"{name}","browser_download_url":"https://x/g","size":42}}]}}"#
        );
        let r = parse_latest(&body, "toon-warfare").unwrap().unwrap();
        assert_eq!(
            (r.version.as_str(), r.url.as_str(), r.size),
            ("999.0.0", "https://x/g", 42)
        );
        // not newer: nothing to do
        let old = body.replace("v999.0.0", "v0.0.1");
        assert!(parse_latest(&old, "toon-warfare").unwrap().is_none());
        // newer but without our file
        assert!(parse_latest(&body, "toon-other").is_err());
    }
}
