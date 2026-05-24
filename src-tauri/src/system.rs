use std::{path::Path, process::Command as StdCommand};

#[cfg(target_os = "windows")]
use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

use tauri::AppHandle;

use crate::config;

#[tauri::command]
pub fn check_admin() -> bool {
    #[cfg(target_os = "windows")]
    {
        let result = StdCommand::new("net").arg("session").output();
        return result
            .map(|output| output.status.success())
            .unwrap_or(false);
    }

    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

#[tauri::command]
pub fn restart_as_admin(
    app: AppHandle,
    enable_tun: Option<bool>,
    resume_proxy: Option<bool>,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if check_admin() {
            if let Some(true) = enable_tun {
                persist_elevation_request(&app, enable_tun, resume_proxy).map_err(to_string_err)?;
            }
            return Ok(());
        }

        persist_elevation_request(&app, enable_tun, resume_proxy).map_err(to_string_err)?;
        let current_exe = std::env::current_exe().map_err(to_string_err)?;
        shell_execute_runas(&current_exe).map_err(to_string_err)?;

        app.exit(0);
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, enable_tun, resume_proxy);
        Err("当前平台暂未实现管理员提权重启".to_string())
    }
}

#[cfg(target_os = "windows")]
pub fn apply_pending_elevation_settings(_app: &AppHandle) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn ensure_admin_on_startup(app: &AppHandle) -> anyhow::Result<()> {
    if check_admin() {
        return Ok(());
    }

    let current_exe = std::env::current_exe()?;
    shell_execute_runas(&current_exe)?;
    app.exit(0);
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn ensure_admin_on_startup(_app: &AppHandle) -> anyhow::Result<()> {
    Ok(())
}

pub fn disable_system_proxy_for_tun() -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::{
            Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
            Networking::WinInet::{
                InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED,
            },
            System::Registry::{
                RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY_CURRENT_USER,
                KEY_SET_VALUE, REG_DWORD,
            },
        };

        unsafe {
            let subkey = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings");
            let mut key = std::ptr::null_mut();
            let open_status = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                KEY_SET_VALUE,
                &mut key,
            );
            if open_status != ERROR_SUCCESS {
                anyhow::bail!("failed to open Windows Internet Settings: {}", open_status);
            }

            let proxy_enable: u32 = 0;
            let set_status = RegSetValueExW(
                key,
                to_wide("ProxyEnable").as_ptr(),
                0,
                REG_DWORD,
                (&proxy_enable as *const u32).cast(),
                std::mem::size_of_val(&proxy_enable) as u32,
            );

            let delete_proxy_server = RegDeleteValueW(key, to_wide("ProxyServer").as_ptr());
            let delete_proxy_override = RegDeleteValueW(key, to_wide("ProxyOverride").as_ptr());
            RegCloseKey(key);

            if set_status != ERROR_SUCCESS {
                anyhow::bail!("failed to disable Windows system proxy: {}", set_status);
            }
            if delete_proxy_server != ERROR_SUCCESS && delete_proxy_server != ERROR_FILE_NOT_FOUND {
                anyhow::bail!(
                    "failed to clear Windows ProxyServer: {}",
                    delete_proxy_server
                );
            }
            if delete_proxy_override != ERROR_SUCCESS
                && delete_proxy_override != ERROR_FILE_NOT_FOUND
            {
                anyhow::bail!(
                    "failed to clear Windows ProxyOverride: {}",
                    delete_proxy_override
                );
            }

            InternetSetOptionW(
                std::ptr::null_mut(),
                INTERNET_OPTION_SETTINGS_CHANGED,
                std::ptr::null_mut(),
                0,
            );
            InternetSetOptionW(
                std::ptr::null_mut(),
                INTERNET_OPTION_REFRESH,
                std::ptr::null_mut(),
                0,
            );
        }
    }

    Ok(())
}

pub fn reset_network_runtime_state(app: &AppHandle) -> anyhow::Result<()> {
    disable_system_proxy_for_tun()?;

    let mut settings = config::load_or_create_settings(app)?;
    let mut changed = false;

    if settings.tun_enabled {
        settings.tun_enabled = false;
        changed = true;
    }
    if settings.proxy_enabled {
        settings.proxy_enabled = false;
        changed = true;
    }
    if settings.resume_after_elevation {
        settings.resume_after_elevation = false;
        changed = true;
    }

    if changed {
        config::save_settings(app, &settings)?;
        config::write_singbox_config(app, &settings)?;
    }
    config::mark_core_runtime_state(app, false)?;

    Ok(())
}

pub fn apply_auto_launch_setting(app: &AppHandle, enabled: bool) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let _ = app;
        set_windows_run_entry(enabled)?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, enabled);
    }

    Ok(())
}

pub fn open_path_in_file_manager(path: &Path) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let status = StdCommand::new("explorer")
            .arg(path)
            .status()
            .map_err(anyhow::Error::from)?;
        if !status.success() {
            anyhow::bail!("failed to open path in Explorer: {}", path.display());
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        anyhow::bail!("当前平台暂未实现打开目录");
    }

    Ok(())
}

pub fn recover_from_unclean_shutdown(app: &AppHandle) -> anyhow::Result<bool> {
    if !config::has_unclean_runtime_marker(app)? {
        return Ok(false);
    }

    reset_network_runtime_state(app)?;
    Ok(true)
}

#[cfg(not(target_os = "windows"))]
pub fn apply_pending_elevation_settings(_app: &AppHandle) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(target_os = "windows")]
fn persist_elevation_request(
    app: &AppHandle,
    enable_tun: Option<bool>,
    resume_proxy: Option<bool>,
) -> anyhow::Result<()> {
    let mut settings = config::load_or_create_settings(app)?;
    let mut changed = false;

    if enable_tun == Some(true) && !settings.tun_enabled {
        settings.tun_enabled = true;
        changed = true;
    }

    if resume_proxy == Some(true) && !settings.proxy_enabled {
        settings.proxy_enabled = true;
        changed = true;
    }

    if resume_proxy == Some(true) && !settings.resume_after_elevation {
        settings.resume_after_elevation = true;
        changed = true;
    }

    if changed {
        config::save_settings(app, &settings)?;
        config::write_singbox_config(app, &settings)?;
    }

    Ok(())
}

fn to_string_err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(target_os = "windows")]
fn set_windows_run_entry(enabled: bool) -> anyhow::Result<()> {
    use windows_sys::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
        System::Registry::{
            RegCloseKey, RegCreateKeyW, RegDeleteValueW, RegSetValueExW, HKEY_CURRENT_USER, REG_SZ,
        },
    };

    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const RUN_VALUE_NAME: &str = "WepboxProxyClient";

    unsafe {
        let mut key = std::ptr::null_mut();
        let status = RegCreateKeyW(HKEY_CURRENT_USER, to_wide(RUN_KEY).as_ptr(), &mut key);
        if status != ERROR_SUCCESS {
            anyhow::bail!("failed to open Windows Run registry key: {}", status);
        }

        let value_name = to_wide(RUN_VALUE_NAME);
        if enabled {
            let exe_path = std::env::current_exe()?;
            let command = format!("\"{}\"", exe_path.display());
            let command_wide = to_wide(&command);
            let bytes = command_wide.len() * std::mem::size_of::<u16>();
            let set_status = RegSetValueExW(
                key,
                value_name.as_ptr(),
                0,
                REG_SZ,
                command_wide.as_ptr().cast(),
                bytes as u32,
            );
            RegCloseKey(key);

            if set_status != ERROR_SUCCESS {
                anyhow::bail!("failed to enable auto launch: {}", set_status);
            }
        } else {
            let delete_status = RegDeleteValueW(key, value_name.as_ptr());
            RegCloseKey(key);

            if delete_status != ERROR_SUCCESS && delete_status != ERROR_FILE_NOT_FOUND {
                anyhow::bail!("failed to disable auto launch: {}", delete_status);
            }
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn shell_execute_runas(exe_path: &Path) -> anyhow::Result<()> {
    use windows_sys::Win32::{
        Foundation::HWND,
        UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    };

    let operation = to_wide("runas");
    let file = to_wide_os(exe_path.as_os_str());

    let result = unsafe {
        ShellExecuteW(
            HWND::default(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;

    if result <= 32 {
        anyhow::bail!(
            "failed to request administrator elevation, shell execute code {}",
            result
        );
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn to_wide(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(target_os = "windows")]
fn to_wide_os(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}
