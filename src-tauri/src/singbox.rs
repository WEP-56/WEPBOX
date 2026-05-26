use std::process::Command as StdCommand;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex as StdMutex,
};
use std::{
    collections::HashSet,
    fs,
    io::{Cursor, Read, Seek, SeekFrom, Write},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::{
    process::{CommandChild, CommandEvent},
    ShellExt,
};

use crate::{config, models::SingboxReleaseInfo, system::reset_network_runtime_state};

const MAX_LOG_SIZE_BYTES: u64 = 32 * 1024 * 1024;
const KEEP_LOG_TAIL_BYTES: usize = 4 * 1024 * 1024;
const SINGBOX_RELEASES_API: &str =
    "https://api.github.com/repos/SagerNet/sing-box/releases?per_page=20";
const SINGBOX_RELEASES_PAGE: &str = "https://github.com/SagerNet/sing-box/releases";
const GITHUB_USER_AGENT: &str = "wepbox-singbox-client";

pub struct SingboxManager {
    child: Option<CommandChild>,
    runtime: Arc<CoreRuntimeState>,
}

pub struct CoreRuntimeSnapshot {
    pub running: bool,
    pub last_exit: Option<String>,
    pub started_at: Option<u64>,
}

#[derive(Default)]
struct CoreRuntimeState {
    running: AtomicBool,
    stopping: AtomicBool,
    started_at: AtomicU64,
    last_exit: StdMutex<Option<String>>,
}

impl Default for SingboxManager {
    fn default() -> Self {
        Self {
            child: None,
            runtime: Arc::new(CoreRuntimeState::default()),
        }
    }
}

impl SingboxManager {
    pub fn is_running(&self) -> bool {
        self.runtime.running.load(Ordering::SeqCst)
    }

    pub fn snapshot(&self) -> CoreRuntimeSnapshot {
        let started_at = match self.runtime.started_at.load(Ordering::SeqCst) {
            0 => None,
            value => Some(value),
        };

        CoreRuntimeSnapshot {
            running: self.is_running(),
            last_exit: self
                .runtime
                .last_exit
                .lock()
                .ok()
                .and_then(|value| value.clone()),
            started_at,
        }
    }

    pub async fn start(&mut self, app: &AppHandle, config_path: PathBuf) -> Result<()> {
        if self.is_running() {
            return Ok(());
        }
        self.child.take();

        if !config_path.exists() {
            bail!("sing-box config does not exist: {}", config_path.display());
        }

        cap_singbox_log_file(app)?;

        for path in sidecar_candidate_paths(app)? {
            let size = std::fs::metadata(&path)
                .map(|metadata| metadata.len())
                .unwrap_or_default();
            if size < 1024 {
                bail!(
                    "sing-box sidecar is still a placeholder, replace it with the real binary: {}",
                    path.display()
                );
            }
            cleanup_sidecar_process_by_path(&path)?;
        }

        let config_arg = config_path
            .to_str()
            .context("sing-box config path is not valid UTF-8")?
            .to_owned();

        validate_singbox_config(app, &config_arg)?;

        let (mut rx, child) = app
            .shell()
            .sidecar("sing-box")
            .context("failed to create sing-box sidecar command")?
            .args(["run", "-c", &config_arg])
            .spawn()
            .context("failed to spawn sing-box sidecar")?;

        self.runtime.running.store(true, Ordering::SeqCst);
        self.runtime.stopping.store(false, Ordering::SeqCst);
        self.runtime
            .started_at
            .store(now_unix_seconds(), Ordering::SeqCst);
        replace_last_exit(&self.runtime, None);

        let runtime = Arc::clone(&self.runtime);
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    CommandEvent::Stdout(bytes) => {
                        print_singbox_line("stdout", &bytes);
                    }
                    CommandEvent::Stderr(bytes) => {
                        print_singbox_line("stderr", &bytes);
                    }
                    CommandEvent::Error(error) => {
                        eprintln!("[sing-box:error] {error}");
                    }
                    CommandEvent::Terminated(payload) => {
                        let message = format!(
                            "sing-box terminated: code={:?}, signal={:?}",
                            payload.code, payload.signal
                        );
                        println!("[sing-box] {message}");
                        runtime.running.store(false, Ordering::SeqCst);
                        runtime.started_at.store(0, Ordering::SeqCst);

                        if runtime.stopping.swap(false, Ordering::SeqCst) {
                            replace_last_exit(&runtime, None);
                        } else {
                            replace_last_exit(&runtime, Some(message.clone()));
                            if let Err(error) = recover_after_unexpected_exit(&app_handle) {
                                eprintln!("[sing-box:recovery] {error}");
                            }
                        }
                    }
                    _ => {
                        println!("[sing-box] {event:?}");
                    }
                }
            }
        });

        self.child = Some(child);
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        let kill_error = if let Some(child) = self.child.take() {
            self.runtime.stopping.store(true, Ordering::SeqCst);
            child
                .kill()
                .context("failed to kill sing-box sidecar")
                .err()
        } else {
            None
        };
        self.runtime.running.store(false, Ordering::SeqCst);
        self.runtime.started_at.store(0, Ordering::SeqCst);
        replace_last_exit(&self.runtime, None);
        if let Some(error) = kill_error {
            return Err(error);
        }
        Ok(())
    }
}

fn print_singbox_line(stream: &str, bytes: &[u8]) {
    let text = String::from_utf8_lossy(bytes);
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        println!("[sing-box:{stream}] {line}");
    }
}

pub fn cleanup_existing_sidecar(app: &AppHandle) -> Result<()> {
    for path in sidecar_candidate_paths(app)? {
        cleanup_sidecar_process_by_path(&path)?;
    }
    Ok(())
}

pub fn cleanup_runtime_on_exit(app: &AppHandle) -> Result<()> {
    let cleanup_result = cleanup_existing_sidecar(app);
    let reset_result = reset_network_runtime_state(app);
    cleanup_result?;
    reset_result?;
    Ok(())
}

pub fn validate_current_config(app: &AppHandle) -> Result<PathBuf> {
    let config_path = config::singbox_config_path(app)?;
    let config_arg = config_path
        .to_str()
        .context("sing-box config path is not valid UTF-8")?;
    validate_singbox_config(app, config_arg)?;
    Ok(config_path)
}

pub fn primary_sidecar_path(app: &AppHandle) -> Result<PathBuf> {
    sidecar_candidate_paths(app)?
        .into_iter()
        .next()
        .context("failed to resolve sing-box sidecar path")
}

pub fn query_sidecar_version(app: &AppHandle) -> Result<String> {
    let path = primary_sidecar_path(app)?;
    query_binary_version(&path)
}

pub async fn list_singbox_releases() -> Result<Vec<SingboxReleaseInfo>> {
    let mut items = match fetch_singbox_releases().await {
        Ok(releases) => releases
            .into_iter()
            .filter(|release| !release.draft && !release.prerelease)
            .filter_map(|release| release.into_release_info())
            .collect::<Vec<_>>(),
        Err(error) => {
            eprintln!(
                "[sing-box:update] GitHub API failed, falling back to releases page: {error}"
            );
            Vec::new()
        }
    };

    if items.is_empty() {
        items = fetch_singbox_releases_from_page().await?;
    }

    if items.is_empty() {
        bail!("failed to find a Windows sing-box release asset");
    }

    Ok(items)
}

pub async fn install_singbox_release(app: &AppHandle, version: &str) -> Result<(PathBuf, String)> {
    let normalized_version = normalize_release_version(version);
    if normalized_version.is_empty() {
        bail!("sing-box version is required");
    }

    let download_url = match find_release_download_url(&normalized_version).await {
        Ok(url) => url,
        Err(error) => {
            eprintln!("[sing-box:update] GitHub API asset lookup failed, using deterministic release URL: {error}");
            release_download_url(&normalized_version)
        }
    };

    let bytes = download_release_asset(&download_url).await?;
    let exe = extract_singbox_exe(&bytes)?;

    cleanup_existing_sidecar(app)?;
    let target = primary_sidecar_path(app)?;
    let target_dir = target
        .parent()
        .map(std::path::Path::to_path_buf)
        .context("failed to resolve sing-box sidecar directory")?;
    fs::create_dir_all(&target_dir).context("failed to create sidecar directory")?;

    let tmp = target.with_file_name(format!(
        "{}.download",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("sing-box.exe")
    ));
    fs::write(&tmp, exe).context("failed to write downloaded sing-box binary")?;
    let _ = query_binary_version(&tmp).context("downloaded sing-box binary is not runnable")?;

    let backup = target.with_file_name(format!(
        "{}.bak",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("sing-box.exe")
    ));

    if target.exists() {
        let _ = fs::remove_file(&backup);
        fs::copy(&target, &backup).context("failed to create sing-box backup")?;
        fs::remove_file(&target).context("failed to remove old sing-box binary")?;
    }

    if let Err(error) = fs::rename(&tmp, &target) {
        if backup.exists() {
            let _ = fs::copy(&backup, &target);
        }
        let _ = fs::remove_file(&tmp);
        return Err(error).context("failed to replace sing-box binary");
    }

    let installed_version = query_binary_version(&target)?;
    Ok((target, installed_version))
}

fn query_binary_version(path: &std::path::Path) -> Result<String> {
    let mut command = StdCommand::new(path);
    command.arg("version");
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);

    let output = command
        .output()
        .with_context(|| format!("failed to query sing-box version with {}", path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        bail!(
            "failed to query sing-box version: {}",
            if stderr.is_empty() {
                output.status.to_string()
            } else {
                stderr
            }
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = format_singbox_version(&stdout);
    if version.is_empty() {
        bail!("sing-box version output is empty");
    }
    Ok(version)
}

fn format_singbox_version(output: &str) -> String {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut tokens = trimmed.split_whitespace();
    while let Some(token) = tokens.next() {
        if token.eq_ignore_ascii_case("version") {
            if let Some(version) = tokens.next() {
                return format!("version: {}", version.trim_start_matches('v'));
            }
        }
    }

    trimmed
        .split_whitespace()
        .find(|token| {
            token
                .trim_start_matches('v')
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.')
        })
        .map(|version| format!("version: {}", version.trim_start_matches('v')))
        .unwrap_or_else(|| trimmed.lines().next().unwrap_or_default().to_string())
}

async fn fetch_singbox_releases() -> Result<Vec<GitHubRelease>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(25))
        .user_agent(GITHUB_USER_AGENT)
        .build()
        .context("failed to create GitHub client")?;

    let response = client
        .get(SINGBOX_RELEASES_API)
        .send()
        .await
        .context("failed to request sing-box releases from GitHub")?;

    if !response.status().is_success() {
        bail!("GitHub releases request failed: {}", response.status());
    }

    response
        .json::<Vec<GitHubRelease>>()
        .await
        .context("failed to parse sing-box releases")
}

async fn fetch_singbox_releases_from_page() -> Result<Vec<SingboxReleaseInfo>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(25))
        .user_agent(GITHUB_USER_AGENT)
        .build()
        .context("failed to create GitHub page client")?;

    let response = client
        .get(SINGBOX_RELEASES_PAGE)
        .send()
        .await
        .context("failed to request sing-box releases page")?;

    if !response.status().is_success() {
        bail!("GitHub releases page request failed: {}", response.status());
    }

    let html = response
        .text()
        .await
        .context("failed to read sing-box releases page")?;
    let mut versions = Vec::new();
    let mut seen = HashSet::new();
    let marker = "/SagerNet/sing-box/releases/tag/";
    let mut rest = html.as_str();

    while let Some(index) = rest.find(marker) {
        let after = &rest[index + marker.len()..];
        let tag = after
            .split(|c| c == '"' || c == '\'' || c == '<' || c == '?' || c == '#')
            .next()
            .unwrap_or_default();
        let version = normalize_release_version(tag);
        if !version.is_empty()
            && !version.contains("alpha")
            && !version.contains("beta")
            && !version.contains("rc")
            && seen.insert(version.clone())
        {
            versions.push(version);
            if versions.len() >= 20 {
                break;
            }
        }
        rest = after;
    }

    Ok(versions
        .into_iter()
        .map(|version| {
            let tag_name = format!("v{version}");
            SingboxReleaseInfo {
                asset_name: release_asset_name(&version),
                version,
                tag_name,
                published_at: None,
                asset_size: None,
            }
        })
        .collect())
}

async fn find_release_download_url(version: &str) -> Result<String> {
    let releases = fetch_singbox_releases().await?;
    releases
        .into_iter()
        .filter(|release| !release.draft && !release.prerelease)
        .find(|release| {
            normalize_release_version(&release.tag_name) == version
                && release
                    .assets
                    .iter()
                    .any(|asset| target_asset_name(&asset.name))
        })
        .and_then(|release| {
            release
                .assets
                .into_iter()
                .find(|asset| target_asset_name(&asset.name))
                .map(|asset| asset.browser_download_url)
        })
        .with_context(|| format!("sing-box release asset not found: {version}"))
}

async fn download_release_asset(url: &str) -> Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .user_agent(GITHUB_USER_AGENT)
        .build()
        .context("failed to create download client")?;

    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to download sing-box asset: {url}"))?;

    if !response.status().is_success() {
        bail!("sing-box asset download failed: {}", response.status());
    }

    Ok(response
        .bytes()
        .await
        .context("failed to read sing-box download body")?
        .to_vec())
}

fn extract_singbox_exe(bytes: &[u8]) -> Result<Vec<u8>> {
    let reader = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).context("failed to open sing-box zip asset")?;

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .context("failed to read sing-box zip entry")?;
        let name = file.name().replace('\\', "/").to_ascii_lowercase();
        if !file.is_dir() && (name == "sing-box.exe" || name.ends_with("/sing-box.exe")) {
            let mut exe = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut exe)
                .context("failed to extract sing-box.exe from zip")?;
            if exe.len() < 1024 {
                bail!("extracted sing-box.exe is too small");
            }
            return Ok(exe);
        }
    }

    bail!("sing-box.exe was not found in the release zip")
}

fn normalize_release_version(version: &str) -> String {
    version
        .trim()
        .trim_start_matches("sing-box")
        .trim()
        .trim_start_matches('v')
        .to_ascii_lowercase()
}

fn target_asset_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("windows") && lower.contains(target_asset_arch()) && lower.ends_with(".zip")
}

fn target_asset_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "x86" => "386",
        "aarch64" => "arm64",
        _ => "amd64",
    }
}

fn release_asset_name(version: &str) -> String {
    format!("sing-box-{version}-windows-{}.zip", target_asset_arch())
}

fn release_download_url(version: &str) -> String {
    let asset_name = release_asset_name(version);
    format!("https://github.com/SagerNet/sing-box/releases/download/v{version}/{asset_name}")
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    prerelease: bool,
    draft: bool,
    published_at: Option<String>,
    assets: Vec<GitHubAsset>,
}

impl GitHubRelease {
    fn into_release_info(self) -> Option<SingboxReleaseInfo> {
        let asset = self
            .assets
            .into_iter()
            .find(|asset| target_asset_name(&asset.name))?;
        Some(SingboxReleaseInfo {
            version: normalize_release_version(&self.tag_name),
            tag_name: self.tag_name,
            published_at: self.published_at,
            asset_name: asset.name,
            asset_size: asset.size,
        })
    }
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: Option<u64>,
}

fn cap_singbox_log_file(app: &AppHandle) -> Result<()> {
    let path = config::singbox_log_path(app)?;
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).context("failed to read sing-box log metadata"),
    };

    if metadata.len() <= MAX_LOG_SIZE_BYTES {
        return Ok(());
    }

    let mut file = fs::File::open(&path).context("failed to open sing-box log for truncation")?;
    let keep = usize::try_from(metadata.len().min(KEEP_LOG_TAIL_BYTES as u64))
        .unwrap_or(KEEP_LOG_TAIL_BYTES);
    let start = metadata.len().saturating_sub(keep as u64);
    file.seek(SeekFrom::Start(start))
        .context("failed to seek sing-box log tail")?;

    let mut buffer = Vec::with_capacity(keep);
    file.read_to_end(&mut buffer)
        .context("failed to read sing-box log tail")?;

    let notice = format!(
        "[log-maintenance] log file exceeded {} bytes; older content truncated.\n",
        MAX_LOG_SIZE_BYTES
    );
    let mut output = fs::File::create(&path).context("failed to rewrite sing-box log")?;
    output
        .write_all(notice.as_bytes())
        .context("failed to write log truncation notice")?;
    output
        .write_all(&buffer)
        .context("failed to write truncated sing-box log tail")?;
    Ok(())
}

fn validate_singbox_config(app: &AppHandle, config_arg: &str) -> Result<()> {
    let path = primary_sidecar_path(app)?;

    let mut command = StdCommand::new(&path);
    command.args(["check", "-c", config_arg]);
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);

    let output = command
        .output()
        .with_context(|| format!("failed to validate sing-box config with {}", path.display()))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let detail = if stderr.is_empty() { stdout } else { stderr };
    bail!("sing-box config check failed: {detail}");
}

fn recover_after_unexpected_exit(app: &AppHandle) -> Result<()> {
    reset_network_runtime_state(app)
}

fn replace_last_exit(runtime: &CoreRuntimeState, value: Option<String>) {
    if let Ok(mut last_exit) = runtime.last_exit.lock() {
        *last_exit = value;
    }
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn sidecar_candidate_paths(app: &AppHandle) -> Result<Vec<PathBuf>> {
    let mut candidates = Vec::new();

    if let Some(resource_path) = app
        .path()
        .resolve("binaries/sing-box", tauri::path::BaseDirectory::Resource)
        .ok()
        .filter(|path| path.exists())
    {
        candidates.push(resource_path);
    }

    if let Ok(current_exe) = std::env::current_exe() {
        let dev_sidecar = current_exe.parent().map(|dir| {
            dir.join(if cfg!(windows) {
                "sing-box.exe"
            } else {
                "sing-box"
            })
        });
        if let Some(path) = dev_sidecar.filter(|path| path.exists()) {
            if !candidates.iter().any(|existing| existing == &path) {
                candidates.push(path);
            }
        }
    }

    if candidates.is_empty() {
        bail!("failed to resolve sing-box sidecar path");
    }

    Ok(candidates)
}

fn cleanup_sidecar_process_by_path(path: &std::path::Path) -> Result<()> {
    #[cfg(windows)]
    {
        let escaped_path = path.display().to_string().replace('\'', "''");
        let script = format!(
            "Get-CimInstance Win32_Process -Filter \"Name = 'sing-box.exe'\" | Where-Object {{ $_.ExecutablePath -eq '{escaped_path}' }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}"
        );
        let mut command = StdCommand::new("powershell");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ]);
        command.creation_flags(0x08000000);
        let status = command
            .status()
            .context("failed to run powershell to cleanup stale sing-box")?;
        if !status.success() {
            bail!(
                "failed to cleanup stale sing-box process for {}",
                path.display()
            );
        }
    }

    #[cfg(not(windows))]
    {
        let _ = path;
    }

    Ok(())
}

impl Drop for SingboxManager {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            let _ = child.kill();
        }
    }
}
