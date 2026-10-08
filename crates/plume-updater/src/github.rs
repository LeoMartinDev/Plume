use serde::Deserialize;
use std::time::Duration;

use crate::{Result, VERSION};

const DOWNLOAD_BASE: &str = "https://github.com/LeoMartinDev/Plume/releases/download/";

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub url: String,
    pub installer_name: String,
    pub installer_url: String,
    pub checksum_url: String,
    pub size: u64,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

pub(crate) fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into()
}

pub fn check() -> Result<Option<Release>> {
    let mut response = match agent()
        .get("https://api.github.com/repos/LeoMartinDev/Plume/releases/latest")
        .header("User-Agent", concat!("Plume/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
    {
        Ok(response) => response,
        // No public releases yet is an ordinary state for the first version.
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(error) => return Err(format!("Could not check for updates: {error}")),
    };
    let json = response
        .body_mut()
        .with_config()
        .limit(2 * 1024 * 1024)
        .read_to_string()
        .map_err(|error| error.to_string())?;
    select(&json, VERSION, crate::target()?)
}

fn select(json: &str, current: &str, target: &str) -> Result<Option<Release>> {
    let release: GithubRelease = serde_json::from_str(json).map_err(|error| error.to_string())?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version = release
        .tag_name
        .strip_prefix('v')
        .ok_or("Invalid release tag")?;
    let next = semver::Version::parse(version).map_err(|error| error.to_string())?;
    let current = semver::Version::parse(current).map_err(|error| error.to_string())?;
    if !next.pre.is_empty() || !next.build.is_empty() || next <= current {
        return Ok(None);
    }
    let name = installer_name(version, target);
    let find = |name: &str| -> Result<&Asset> {
        let mut assets = release.assets.iter().filter(|asset| asset.name == name);
        let asset = assets
            .next()
            .ok_or("The release is missing a download for this platform.")?;
        if assets.next().is_some()
            || asset.browser_download_url != format!("{DOWNLOAD_BASE}{}/{name}", release.tag_name)
        {
            return Err("The release contains an invalid download URL.".into());
        }
        Ok(asset)
    };
    let archive = find(&name)?;
    let checksum = find(&format!("{name}.sha256"))?;
    if archive.size == 0 || archive.size > 2 * 1024 * 1024 * 1024 || checksum.size > 4096 {
        return Err("The release contains an invalid download size.".into());
    }
    Ok(Some(Release {
        version: version.into(),
        notes: release.body.clone().unwrap_or_default(),
        url: format!(
            "https://github.com/LeoMartinDev/Plume/releases/tag/{}",
            release.tag_name
        ),
        installer_name: name,
        installer_url: archive.browser_download_url.clone(),
        checksum_url: checksum.browser_download_url.clone(),
        size: archive.size,
    }))
}

// Shared contract with scripts/installer.mjs. Only native installers are offered.
fn installer_name(version: &str, target: &str) -> String {
    let extension = if target.contains("windows") {
        "exe"
    } else if target.contains("apple") {
        "dmg"
    } else {
        "deb"
    };
    format!("Plume-v{version}-{target}.{extension}")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(version: &str, target: &str) -> String {
        let name = installer_name(version, target);
        serde_json::json!({"tag_name":format!("v{version}"),"draft":false,"prerelease":false,
            "assets":[{"name":name,"size":1234,"browser_download_url":format!("{DOWNLOAD_BASE}v{version}/{name}")},
            {"name":format!("{name}.sha256"),"size":100,"browser_download_url":format!("{DOWNLOAD_BASE}v{version}/{name}.sha256")}]}).to_string()
    }
    #[test]
    fn selects_each_native_platform_and_compares_versions_numerically() {
        for target in [
            "aarch64-apple-darwin",
            "x86_64-pc-windows-msvc",
            "x86_64-unknown-linux-gnu",
        ] {
            let json = fixture("0.10.0", target);
            assert!(select(&json, "0.9.0", target).unwrap().is_some());
            assert!(select(&json, "0.10.0", target).unwrap().is_none());
            assert!(select(&json, "0.11.0", target).unwrap().is_none());
        }
    }
    #[test]
    fn macos_selects_dmg_assets_and_rejects_legacy_packages() {
        let target = "aarch64-apple-darwin";
        assert_eq!(
            installer_name("0.2.0", target),
            "Plume-v0.2.0-aarch64-apple-darwin.dmg"
        );
        assert!(select(
            &fixture("0.2.0", target).replace(".dmg", ".pkg"),
            "0.1.0",
            target
        )
        .is_err());
    }
    #[test]
    fn rejects_previews_missing_assets_and_foreign_urls() {
        let target = "aarch64-apple-darwin";
        let json = fixture("0.2.0", target);
        assert!(select(
            &json.replace("\"prerelease\":false", "\"prerelease\":true"),
            "0.1.0",
            target
        )
        .unwrap()
        .is_none());
        assert!(select(
            &json.replace("LeoMartinDev/Plume", "someone/else"),
            "0.1.0",
            target
        )
        .is_err());
        assert!(select(&json, "0.1.0", "x86_64-unknown-linux-gnu").is_err());
        assert!(select(&fixture("0.2.0-rc.1", target), "0.1.0", target)
            .unwrap()
            .is_none());
    }
}
