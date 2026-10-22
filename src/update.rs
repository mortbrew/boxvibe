//! Is there a newer release? Like tubevibe: asks GitHub each time the app
//! opens (in the background, `[ui] check_updates`) and, when there is one,
//! says how to get it on this machine. Where the release archive fits and the
//! program's folder is writable, About's Update Now button downloads it and
//! puts the new program in place of the running one ([`install`]).
//! Ported from tubevibe's `update.rs`, pointed at mortbrew/boxvibe.

use std::env::consts::{ARCH, OS};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

pub const REPO: &str = "https://github.com/mortbrew/boxvibe";
const LATEST: &str = "https://api.github.com/repos/mortbrew/boxvibe/releases/latest";
const DOWNLOAD: &str = "https://github.com/mortbrew/boxvibe/releases/download";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    /// `0.2.0`.
    pub version: String,
    /// A command to paste in a terminal, for when Update Now can't.
    pub how: String,
    /// Whether Update Now can put the new version in place here, or why not.
    pub replace: Replace,
}

/// Can boxvibe update itself in place, and if not, why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Replace {
    /// Download this archive and swap its program in for the running one.
    Ready(String),
    /// Running from a cargo `target` folder: build the new version instead.
    FromSource,
    /// Installed with `cargo install`: reinstall the same way.
    Cargo,
    /// Installed by a package manager (`/usr/bin`…): update the package.
    Package,
    /// The program's folder can't be written to.
    ReadOnly,
    /// No release file for this machine.
    NoAsset,
}

impl Replace {
    /// Why Update Now can't, in a few words (`None`: it can).
    pub fn why_not(&self) -> Option<&'static str> {
        match self {
            Self::Ready(_) => None,
            Self::FromSource => Some("Built from source: run git pull && cargo build --release"),
            Self::Cargo => Some("Installed with cargo install: reinstall it the same way"),
            Self::Package => Some("Installed from a package: update the package"),
            Self::ReadOnly => Some("boxvibe's folder isn't writable"),
            Self::NoAsset => Some("No download for this system"),
        }
    }
}

/// Where a self-update stands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Progress {
    #[default]
    Idle,
    /// Downloading: percent done.
    Downloading(u8),
    /// The new version is in place: Restart Now starts it.
    Done(String),
    Failed(String),
}

/// What asking GitHub found out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checked {
    Newer(Update),
    /// GitHub's latest release is this version (or older).
    Latest,
    /// Offline, no release yet, rate limited, or a tag that isn't a version.
    Unknown,
}

/// This build's version, or `BOXVIBE_FAKE_VERSION` (to try the update flow
/// against an existing release, e.g. `BOXVIBE_FAKE_VERSION=0.0.1 boxvibe`).
fn current_version() -> String {
    std::env::var("BOXVIBE_FAKE_VERSION")
        .ok()
        .filter(|v| version(v).is_some())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned())
}

pub fn check() -> Checked {
    let Ok(Some((tag, assets))) = fetch_latest() else { return Checked::Unknown };
    let current = current_version();
    let exe = running_exe();
    match update_for(&current, &tag, &assets, &exe.display().to_string(), OS, ARCH) {
        Some(mut update) => {
            if matches!(update.replace, Replace::Ready(_)) && !exe.parent().is_some_and(writable) {
                update.replace = Replace::ReadOnly;
            }
            Checked::Newer(update)
        }
        None => latest_or_unknown(&current, &tag),
    }
}

/// No newer release: up to date when the tag really is a version.
fn latest_or_unknown(current: &str, tag: &str) -> Checked {
    match (version(tag), version(current)) {
        (Some(latest), Some(mine)) if latest <= mine => Checked::Latest,
        _ => Checked::Unknown,
    }
}

fn running_exe() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    std::fs::canonicalize(&exe).unwrap_or(exe)
}

fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".boxvibe-write-test-{}", std::process::id()));
    let ok = std::fs::File::create(&probe).is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

fn agent(timeout: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout)))
        .user_agent(concat!("boxvibe/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// The latest release's tag and files; `None` when GitHub has nothing to say
/// (no release yet: 404; rate limited: 403, 429).
fn fetch_latest() -> Result<Option<(String, Vec<String>)>, String> {
    let reply = match agent(10).get(LATEST).header("Accept", "application/vnd.github+json").call() {
        Ok(reply) => reply,
        Err(ureq::Error::StatusCode(status)) if no_news(status) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let text = reply.into_body().read_to_string().map_err(|e| e.to_string())?;
    let json: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    parse_latest(&json).map(Some).ok_or_else(|| "GitHub's answer has no version".into())
}

/// GitHub statuses that just mean "nothing to say": no release published
/// yet, or too many requests from this address for now.
fn no_news(status: u16) -> bool {
    matches!(status, 404 | 403 | 429)
}

/// Tag and asset names of GitHub's "latest release" answer.
pub fn parse_latest(json: &Value) -> Option<(String, Vec<String>)> {
    let tag = json["tag_name"].as_str()?.to_owned();
    let assets = json["assets"].as_array()?.iter().filter_map(|a| a["name"].as_str().map(str::to_owned)).collect();
    Some((tag, assets))
}

/// `1.2.3` → `(1, 2, 3)`; a leading `v` and anything after `-` are ignored.
fn version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.trim_start_matches('v').split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse().ok());
    Some((parts.next()??, parts.next().flatten().unwrap_or(0), parts.next().flatten().unwrap_or(0)))
}

/// The release file for a machine, as `.github/workflows/release.yml` names it.
pub fn tarball(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("boxvibe-x86_64-linux.tar.gz"),
        ("linux", "aarch64") => Some("boxvibe-aarch64-linux.tar.gz"),
        _ => None,
    }
}

/// Built with cargo in a checkout: `…/target/release/` or `…/target/debug/`.
pub fn is_dev_build(exe: &str) -> bool {
    let parts: Vec<&str> = exe.split('/').collect();
    parts.iter().position(|p| *p == "target").is_some_and(|t| {
        let between = parts.get(t + 1..parts.len().saturating_sub(1)).unwrap_or_default();
        between.iter().any(|p| matches!(*p, "release" | "debug"))
    })
}

fn from_cargo(exe: &str) -> bool {
    exe.contains(".cargo/bin")
}

/// Put there by a package manager, which should be the one to update it.
pub fn is_package(exe: &str) -> bool {
    const MANAGED: &[&str] = &["/usr/bin/", "/usr/sbin/", "/bin/", "/usr/lib/", "/usr/share/", "/nix/store/", "/snap/"];
    MANAGED.iter().any(|dir| exe.starts_with(dir))
}

pub fn replace_for(exe: &str, os: &str, arch: &str, assets: &[String], tag: &str) -> Replace {
    if is_dev_build(exe) {
        Replace::FromSource
    } else if from_cargo(exe) {
        Replace::Cargo
    } else if is_package(exe) {
        Replace::Package
    } else {
        match tarball(os, arch).filter(|name| assets.iter().any(|a| a == name)) {
            Some(name) => Replace::Ready(format!("{DOWNLOAD}/{tag}/{name}")),
            None => Replace::NoAsset,
        }
    }
}

/// The update for a machine running `current` from `exe`, when `tag` is newer.
pub fn update_for(current: &str, tag: &str, assets: &[String], exe: &str, os: &str, arch: &str) -> Option<Update> {
    if version(tag)? <= version(current)? {
        return None;
    }
    let cargo = format!("cargo install --locked --git {REPO} --tag {tag}");
    let checkout = exe.strip_suffix("/target/release/boxvibe");
    let how = if from_cargo(exe) {
        cargo
    } else if let Some(dir) = checkout {
        format!("cd {dir} && git pull && git checkout {tag} && cargo build --release")
    } else {
        match tarball(os, arch).filter(|name| assets.iter().any(|a| a == name)) {
            // Only the program out of the archive, straight over the one running.
            Some(name) => {
                let dir = Path::new(exe).parent().map_or_else(|| "~/.local/bin".into(), |d| d.display().to_string());
                format!("curl -fsSL {DOWNLOAD}/{tag}/{name} | tar xz -C {dir} --strip-components=1 boxvibe/boxvibe")
            }
            None => cargo,
        }
    };
    let replace = replace_for(exe, os, arch, assets, tag);
    Some(Update { version: tag.trim_start_matches('v').to_owned(), how, replace })
}

const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;

/// Downloads the archive at `url`, takes the program out of it, checks that
/// it runs and says it is `version`, and swaps it in for the running program
/// (a rename, so a failure halfway leaves the old one).
pub fn install(url: &str, version: &str, mut progress: impl FnMut(u8)) -> Result<PathBuf, String> {
    let exe = running_exe();
    let shown = exe.display().to_string();
    if is_dev_build(&shown) {
        return Err("built from source: git pull && cargo build --release".into());
    }
    let dir = exe.parent().ok_or("no folder for the program")?;
    let archive = download(url, &mut progress)?;
    let program = from_tarball(&archive)?;
    let staged = dir.join(format!(".boxvibe-update-{}", std::process::id()));
    let result = stage(&staged, &program)
        .and_then(|()| check_runs(&staged, version))
        .and_then(|()| self_replace::self_replace(&staged).map_err(|e| format!("replacing {shown}: {e}")));
    let _ = std::fs::remove_file(&staged);
    result.map(|()| exe)
}

fn download(url: &str, progress: &mut impl FnMut(u8)) -> Result<Vec<u8>, String> {
    let reply = agent(300).get(url).call().map_err(|e| format!("download: {e}"))?;
    let total = reply.body().content_length().filter(|n| *n > 0);
    let mut reader = reply.into_body().into_with_config().limit(MAX_DOWNLOAD).reader();
    let mut data = Vec::with_capacity(total.unwrap_or(0) as usize);
    let mut chunk = [0u8; 64 * 1024];
    let mut shown = None;
    loop {
        let n = reader.read(&mut chunk).map_err(|e| format!("download: {e}"))?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&chunk[..n]);
        if let Some(total) = total {
            let percent = (data.len() as u64 * 100 / total).min(100) as u8;
            if shown != Some(percent) {
                shown = Some(percent);
                progress(percent);
            }
        }
    }
    if data.is_empty() {
        return Err("download: empty file".into());
    }
    Ok(data)
}

/// The program out of a release tarball (`boxvibe/boxvibe`).
pub fn from_tarball(gz: &[u8]) -> Result<Vec<u8>, String> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(gz));
    for entry in archive.entries().map_err(|e| format!("archive: {e}"))? {
        let mut entry = entry.map_err(|e| format!("archive: {e}"))?;
        let path = entry.path().map_err(|e| format!("archive: {e}"))?.into_owned();
        if entry.header().entry_type().is_file()
            && path.strip_prefix("./").unwrap_or(&path) == Path::new("boxvibe/boxvibe")
        {
            let mut program = Vec::new();
            entry.read_to_end(&mut program).map_err(|e| format!("archive: {e}"))?;
            return Ok(program);
        }
    }
    Err("archive: no boxvibe/boxvibe inside".into())
}

fn stage(path: &Path, program: &[u8]) -> Result<(), String> {
    std::fs::write(path, program).map_err(|e| format!("writing {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("making {} runnable: {e}", path.display()))?;
    }
    Ok(())
}

/// The new program starts here and says it is `version`.
fn check_runs(program: &Path, version: &str) -> Result<(), String> {
    let out = std::process::Command::new(program)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("the new version does not start here: {e}"))?;
    let said = String::from_utf8_lossy(&out.stdout);
    if out.status.success() && said.split_whitespace().any(|w| w == version) {
        Ok(())
    } else {
        Err(format!("the new version says {:?}, not {version}", said.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> Vec<String> {
        vec!["boxvibe-x86_64-linux.tar.gz".into(), "boxvibe-aarch64-linux.tar.gz".into()]
    }

    #[test]
    fn only_newer_versions_count() {
        let exe = "/home/t/.local/bin/boxvibe";
        assert!(update_for("0.1.0", "v0.2.0", &assets(), exe, "linux", "x86_64").is_some());
        assert!(update_for("0.2.0", "v0.2.0", &assets(), exe, "linux", "x86_64").is_none());
        assert!(update_for("0.3.0", "v0.2.0", &assets(), exe, "linux", "x86_64").is_none());
        assert!(update_for("0.1.0", "nightly", &assets(), exe, "linux", "x86_64").is_none());
        assert_eq!(version("v1.2"), Some((1, 2, 0)));
    }

    #[test]
    fn the_update_matches_how_it_was_installed() {
        let u = |exe: &str| update_for("0.1.0", "v0.2.0", &assets(), exe, "linux", "x86_64").unwrap();
        let local = u("/home/t/.local/bin/boxvibe");
        assert_eq!(local.replace, Replace::Ready(format!("{DOWNLOAD}/v0.2.0/boxvibe-x86_64-linux.tar.gz")));
        assert!(local.how.contains("tar xz -C /home/t/.local/bin"), "{}", local.how);
        let built = u("/home/t/src/boxvibe/target/release/boxvibe");
        assert_eq!(built.replace, Replace::FromSource);
        assert_eq!(built.how, "cd /home/t/src/boxvibe && git pull && git checkout v0.2.0 && cargo build --release");
        assert_eq!(u("/home/t/.cargo/bin/boxvibe").replace, Replace::Cargo);
        assert_eq!(u("/usr/bin/boxvibe").replace, Replace::Package);
        let arm = update_for("0.1.0", "v0.2.0", &assets(), "/opt/b/boxvibe", "linux", "riscv64").unwrap();
        assert_eq!(arm.replace, Replace::NoAsset);
        assert!(arm.how.starts_with("cargo install"));
    }

    #[test]
    fn reads_githubs_answer() {
        let json = serde_json::json!({ "tag_name": "v0.2.0", "assets": [{ "name": "boxvibe-x86_64-linux.tar.gz" }] });
        assert_eq!(parse_latest(&json), Some(("v0.2.0".into(), vec!["boxvibe-x86_64-linux.tar.gz".into()])));
        assert_eq!(parse_latest(&serde_json::json!({})), None);
    }

    #[test]
    fn the_program_comes_out_of_the_archive() {
        let mut tar = tar::Builder::new(Vec::new());
        for (path, data) in [("boxvibe/README.md", &b"readme"[..]), ("boxvibe/boxvibe", &b"\x7fELF"[..])] {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(&mut header, path, data).unwrap();
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gz, &tar.into_inner().unwrap()).unwrap();
        assert_eq!(from_tarball(&gz.finish().unwrap()).unwrap(), b"\x7fELF");
        assert!(from_tarball(b"junk").is_err());
    }

    #[test]
    fn up_to_date_only_on_a_real_version() {
        assert_eq!(latest_or_unknown("0.2.0", "v0.2.0"), Checked::Latest);
        assert_eq!(latest_or_unknown("0.2.0", "v0.1.0"), Checked::Latest);
        assert_eq!(latest_or_unknown("0.2.0", "nightly"), Checked::Unknown);
        assert!(no_news(404) && no_news(403) && no_news(429) && !no_news(500));
    }

    #[test]
    fn dev_builds_are_recognised() {
        assert!(is_dev_build("/w/boxvibe/target/release/boxvibe"));
        assert!(is_dev_build("/w/boxvibe/target/x86_64-unknown-linux-gnu/debug/boxvibe"));
        assert!(!is_dev_build("/home/t/.local/bin/boxvibe"));
    }
}
