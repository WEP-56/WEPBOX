use std::process::Command as StdCommand;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex as StdMutex,
};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use anyhow::{bail, Context, Result};
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::{
    process::{CommandChild, CommandEvent},
    ShellExt,
};

use crate::{config, system::reset_network_runtime_state};

const MAX_LOG_SIZE_BYTES: u64 = 32 * 1024 * 1024;
const KEEP_LOG_TAIL_BYTES: usize = 4 * 1024 * 1024;

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
    let mut command = StdCommand::new(&path);
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
    let version = stdout.trim();
    if version.is_empty() {
        bail!("sing-box version output is empty");
    }
    Ok(version.to_owned())
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
