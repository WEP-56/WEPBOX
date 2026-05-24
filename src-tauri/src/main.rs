mod clash_api;
mod config;
mod models;
mod singbox;
mod subscriptions;
mod system;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::{fs, time::{SystemTime, UNIX_EPOCH}};

use models::{
    AppSettings, AppStatus, ImportSubscriptionRequest, ImportSubscriptionResult,
    MaintenanceActionResult, MaintenanceInfo, ProxyList, SelectProxyRequest,
};
use singbox::SingboxManager;
use system::{
    apply_auto_launch_setting, check_admin, disable_system_proxy_for_tun,
    open_path_in_file_manager, recover_from_unclean_shutdown, reset_network_runtime_state,
    restart_as_admin,
};
use tauri::{
    image::Image,
    menu::MenuBuilder,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, State, WindowEvent,
};
use tokio::sync::Mutex;

struct AppState {
    core: Mutex<SingboxManager>,
    quit_requested: AtomicBool,
}

type SharedState = Arc<AppState>;

#[tauri::command]
async fn get_settings(app: AppHandle) -> Result<AppSettings, String> {
    config::load_or_create_settings(&app)
        .map(normalize_settings_for_save)
        .map_err(to_err)
}

#[tauri::command]
async fn save_settings(app: AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    let settings = normalize_settings_for_save(settings);
    apply_auto_launch_setting(&app, settings.auto_launch).map_err(to_err)?;
    config::save_settings(&app, &settings).map_err(to_err)?;
    config::write_singbox_config(&app, &settings).map_err(to_err)?;
    Ok(settings)
}

#[tauri::command]
async fn maintenance_info(app: AppHandle) -> Result<MaintenanceInfo, String> {
    let app_data_dir = config::app_data_dir(&app).map_err(to_err)?;
    let settings_path = config::settings_path(&app).map_err(to_err)?;
    let config_path = config::singbox_config_path(&app).map_err(to_err)?;
    let log_path = config::singbox_log_path(&app).map_err(to_err)?;
    let runtime_marker_path = config::core_runtime_marker_path(&app).map_err(to_err)?;
    let subscriptions_dir = config::subscriptions_dir(&app).map_err(to_err)?;
    let sidecar_path = singbox::primary_sidecar_path(&app)
        .ok()
        .map(|path| path.display().to_string());
    let sidecar_version = singbox::query_sidecar_version(&app).ok();

    Ok(MaintenanceInfo {
        app_data_dir: app_data_dir.display().to_string(),
        settings_path: settings_path.display().to_string(),
        config_path: config_path.display().to_string(),
        log_path: log_path.display().to_string(),
        runtime_marker_path: runtime_marker_path.display().to_string(),
        subscriptions_dir: subscriptions_dir.display().to_string(),
        sidecar_path,
        sidecar_version,
    })
}

#[tauri::command]
async fn open_app_data_dir(app: AppHandle) -> Result<MaintenanceActionResult, String> {
    let path = config::app_data_dir(&app).map_err(to_err)?;
    open_path_in_file_manager(&path).map_err(to_err)?;
    Ok(MaintenanceActionResult {
        message: "已打开应用数据目录".to_string(),
        path: Some(path.display().to_string()),
    })
}

#[tauri::command]
async fn open_log_dir(app: AppHandle) -> Result<MaintenanceActionResult, String> {
    let path = config::singbox_log_path(&app).map_err(to_err)?;
    let dir = path
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| "failed to resolve log directory".to_string())?;
    open_path_in_file_manager(&dir).map_err(to_err)?;
    Ok(MaintenanceActionResult {
        message: "已打开日志目录".to_string(),
        path: Some(dir.display().to_string()),
    })
}

#[tauri::command]
async fn clear_singbox_log(app: AppHandle) -> Result<MaintenanceActionResult, String> {
    let path = config::singbox_log_path(&app).map_err(to_err)?;
    if path.exists() {
        fs::write(&path, "").map_err(|error| error.to_string())?;
    }
    Ok(MaintenanceActionResult {
        message: "已清理 sing-box 日志".to_string(),
        path: Some(path.display().to_string()),
    })
}

#[tauri::command]
async fn clear_runtime_marker(app: AppHandle) -> Result<MaintenanceActionResult, String> {
    let path = config::core_runtime_marker_path(&app).map_err(to_err)?;
    if path.exists() {
        fs::remove_file(&path).map_err(|error| error.to_string())?;
    }
    Ok(MaintenanceActionResult {
        message: "已清理 runtime marker".to_string(),
        path: Some(path.display().to_string()),
    })
}

#[tauri::command]
async fn reset_network_state(
    app: AppHandle,
    state: State<'_, SharedState>,
) -> Result<MaintenanceActionResult, String> {
    {
        let mut core = state.core.lock().await;
        let _ = core.stop().await;
    }
    reset_network_runtime_state(&app).map_err(to_err)?;
    Ok(MaintenanceActionResult {
        message: "已清理系统代理残留并回退运行态".to_string(),
        path: None,
    })
}

#[tauri::command]
async fn validate_current_config(app: AppHandle) -> Result<MaintenanceActionResult, String> {
    let path = singbox::validate_current_config(&app).map_err(to_err)?;
    Ok(MaintenanceActionResult {
        message: "当前 sing-box 配置检查通过".to_string(),
        path: Some(path.display().to_string()),
    })
}

#[tauri::command]
async fn export_diagnostics(app: AppHandle) -> Result<MaintenanceActionResult, String> {
    let app_data_dir = config::app_data_dir(&app).map_err(to_err)?;
    let diagnostics_dir = app_data_dir.join("diagnostics");
    fs::create_dir_all(&diagnostics_dir).map_err(|error| error.to_string())?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let report_path = diagnostics_dir.join(format!("diagnostics-{timestamp}.txt"));

    let settings = config::load_or_create_settings(&app).map_err(to_err)?;
    let config_path = config::singbox_config_path(&app).map_err(to_err)?;
    let settings_path = config::settings_path(&app).map_err(to_err)?;
    let log_path = config::singbox_log_path(&app).map_err(to_err)?;
    let runtime_marker_path = config::core_runtime_marker_path(&app).map_err(to_err)?;
    let sidecar_path = singbox::primary_sidecar_path(&app)
        .ok()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "<unavailable>".to_string());
    let sidecar_version = singbox::query_sidecar_version(&app)
        .unwrap_or_else(|error| format!("unavailable: {error}"));
    let config_snapshot = fs::read_to_string(&config_path).unwrap_or_else(|error| {
        format!("failed to read current config: {error}")
    });
    let settings_snapshot = serde_json::to_string_pretty(&settings)
        .unwrap_or_else(|error| format!("failed to serialize settings: {error}"));

    let report = format!(
        "Wepbox diagnostics\nGeneratedAtUnix: {timestamp}\n\nPaths\n- appDataDir: {}\n- settingsPath: {}\n- configPath: {}\n- logPath: {}\n- runtimeMarkerPath: {}\n- sidecarPath: {}\n\nSidecar\n- version: {}\n\nSettings\n{}\n\nConfig\n{}\n",
        app_data_dir.display(),
        settings_path.display(),
        config_path.display(),
        log_path.display(),
        runtime_marker_path.display(),
        sidecar_path,
        sidecar_version,
        settings_snapshot,
        config_snapshot
    );

    fs::write(&report_path, report).map_err(|error| error.to_string())?;

    Ok(MaintenanceActionResult {
        message: "诊断信息已导出".to_string(),
        path: Some(report_path.display().to_string()),
    })
}

#[tauri::command]
async fn app_status(app: AppHandle, state: State<'_, SharedState>) -> Result<AppStatus, String> {
    app_status_inner(app, state.inner().clone()).await
}

async fn app_status_inner(app: AppHandle, state: SharedState) -> Result<AppStatus, String> {
    let settings = config::load_or_create_settings(&app).map_err(to_err)?;
    let core_snapshot = {
        let core = state.core.lock().await;
        core.snapshot()
    };
    let core_healthy = if core_snapshot.running {
        clash_api::Client::from_settings(&settings)
            .list_proxies()
            .await
            .is_ok()
    } else {
        false
    };

    Ok(AppStatus {
        core_running: core_snapshot.running,
        core_healthy,
        core_last_exit: core_snapshot.last_exit,
        core_started_at: core_snapshot.started_at,
        api_base_url: settings.api_base_url(),
        local_mixed_port: settings.local_mixed_port,
        tun_enabled: settings.tun_enabled,
        proxy_enabled: settings.proxy_enabled,
        mode: settings.mode,
    })
}

#[tauri::command]
async fn start_core(app: AppHandle, state: State<'_, SharedState>) -> Result<AppStatus, String> {
    start_core_inner(app, state.inner().clone()).await
}

async fn start_core_inner(app: AppHandle, state: SharedState) -> Result<AppStatus, String> {
    let mut settings = config::load_or_create_settings(&app).map_err(to_err)?;
    config::write_singbox_config(&app, &settings).map_err(to_err)?;
    if settings.tun_enabled {
        disable_system_proxy_for_tun().map_err(to_err)?;
    }

    let config_path = config::singbox_config_path(&app).map_err(to_err)?;
    let mut core = state.core.lock().await;
    core.start(&app, config_path).await.map_err(to_err)?;
    drop(core);

    if let Err(error) = clash_api::Client::from_settings(&settings)
        .wait_until_ready()
        .await
    {
        {
            let mut core = state.core.lock().await;
            let _ = core.stop().await;
        }
        let _ = config::mark_core_runtime_state(&app, false);
        let _ = disable_system_proxy_for_tun();
        settings.proxy_enabled = false;
        let _ = config::save_settings(&app, &settings);
        return Err(to_err(error));
    }

    config::mark_core_runtime_state(&app, true).map_err(to_err)?;
    settings.proxy_enabled = true;
    config::save_settings(&app, &settings).map_err(to_err)?;
    app_status_inner(app, state).await
}

#[tauri::command]
async fn stop_core(app: AppHandle, state: State<'_, SharedState>) -> Result<AppStatus, String> {
    stop_core_inner(app, state.inner().clone()).await
}

async fn stop_core_inner(app: AppHandle, state: SharedState) -> Result<AppStatus, String> {
    let mut core = state.core.lock().await;
    core.stop().await.map_err(to_err)?;
    drop(core);

    config::mark_core_runtime_state(&app, false).map_err(to_err)?;
    disable_system_proxy_for_tun().map_err(to_err)?;

    let mut settings = config::load_or_create_settings(&app).map_err(to_err)?;
    settings.proxy_enabled = false;
    config::save_settings(&app, &settings).map_err(to_err)?;
    app_status_inner(app, state).await
}

#[tauri::command]
async fn restart_core(app: AppHandle, state: State<'_, SharedState>) -> Result<AppStatus, String> {
    restart_core_inner(app, state.inner().clone()).await
}

async fn restart_core_inner(app: AppHandle, state: SharedState) -> Result<AppStatus, String> {
    {
        let mut core = state.core.lock().await;
        core.stop().await.map_err(to_err)?;
    }

    start_core_inner(app, state).await
}

#[tauri::command]
async fn list_proxies(app: AppHandle) -> Result<ProxyList, String> {
    let settings = config::load_or_create_settings(&app).map_err(to_err)?;
    clash_api::Client::from_settings(&settings)
        .list_proxies()
        .await
        .map_err(to_err)
}

#[tauri::command]
async fn select_proxy(app: AppHandle, request: SelectProxyRequest) -> Result<(), String> {
    let settings = config::load_or_create_settings(&app).map_err(to_err)?;
    clash_api::Client::from_settings(&settings)
        .select_proxy(&request.group, &request.name)
        .await
        .map_err(to_err)
}

#[tauri::command]
async fn delay_proxy(app: AppHandle, name: String) -> Result<u64, String> {
    let settings = config::load_or_create_settings(&app).map_err(to_err)?;
    clash_api::Client::from_settings(&settings)
        .delay_proxy(&name)
        .await
        .map_err(to_err)
}

#[tauri::command]
async fn import_subscription(
    app: AppHandle,
    state: State<'_, SharedState>,
    request: ImportSubscriptionRequest,
) -> Result<ImportSubscriptionResult, String> {
    let was_running = {
        let core = state.core.lock().await;
        core.is_running()
    };

    let subscription = subscriptions::import_subscription(&app, request)
        .await
        .map_err(to_err)?;
    let node_count = subscription.node_count;
    let mut restarted = false;

    if was_running {
        {
            let mut core = state.core.lock().await;
            core.stop().await.map_err(to_err)?;
        }

        let mut settings = config::load_or_create_settings(&app).map_err(to_err)?;
        let config_path = config::singbox_config_path(&app).map_err(to_err)?;

        {
            let mut core = state.core.lock().await;
            core.start(&app, config_path).await.map_err(to_err)?;
        }

        clash_api::Client::from_settings(&settings)
            .wait_until_ready()
            .await
            .map_err(to_err)?;

        config::mark_core_runtime_state(&app, true).map_err(to_err)?;
        settings.proxy_enabled = true;
        config::save_settings(&app, &settings).map_err(to_err)?;
        restarted = true;
    }

    Ok(ImportSubscriptionResult {
        subscription,
        node_count,
        restarted,
    })
}

#[tauri::command]
async fn refresh_subscription(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: String,
) -> Result<ImportSubscriptionResult, String> {
    let was_running = {
        let core = state.core.lock().await;
        core.is_running()
    };

    let subscription = subscriptions::refresh_subscription(&app, &id)
        .await
        .map_err(to_err)?;
    let node_count = subscription.node_count;
    let mut restarted = false;

    if was_running {
        {
            let mut core = state.core.lock().await;
            core.stop().await.map_err(to_err)?;
        }

        let mut settings = config::load_or_create_settings(&app).map_err(to_err)?;
        let config_path = config::singbox_config_path(&app).map_err(to_err)?;

        {
            let mut core = state.core.lock().await;
            core.start(&app, config_path).await.map_err(to_err)?;
        }

        clash_api::Client::from_settings(&settings)
            .wait_until_ready()
            .await
            .map_err(to_err)?;

        config::mark_core_runtime_state(&app, true).map_err(to_err)?;
        settings.proxy_enabled = true;
        config::save_settings(&app, &settings).map_err(to_err)?;
        restarted = true;
    }

    Ok(ImportSubscriptionResult {
        subscription,
        node_count,
        restarted,
    })
}

#[tauri::command]
async fn delete_subscription(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), String> {
    let was_running = {
        let core = state.core.lock().await;
        core.is_running()
    };

    subscriptions::delete_subscription(&app, &id).map_err(to_err)?;

    if was_running {
        {
            let mut core = state.core.lock().await;
            core.stop().await.map_err(to_err)?;
        }

        let mut settings = config::load_or_create_settings(&app).map_err(to_err)?;
        let config_path = config::singbox_config_path(&app).map_err(to_err)?;

        {
            let mut core = state.core.lock().await;
            core.start(&app, config_path).await.map_err(to_err)?;
        }

        clash_api::Client::from_settings(&settings)
            .wait_until_ready()
            .await
            .map_err(to_err)?;

        config::mark_core_runtime_state(&app, true).map_err(to_err)?;
        settings.proxy_enabled = true;
        config::save_settings(&app, &settings).map_err(to_err)?;
    }

    Ok(())
}

#[tauri::command]
async fn enter_background_mode(app: AppHandle) -> Result<(), String> {
    hide_main_window(&app).map_err(to_err)
}

fn show_main_window(app: &AppHandle) -> anyhow::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        window.show()?;
        window.set_focus()?;
    }
    Ok(())
}

fn hide_main_window(app: &AppHandle) -> anyhow::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        window.hide()?;
    }
    Ok(())
}

fn setup_tray(app: &tauri::App) -> anyhow::Result<()> {
    let menu = MenuBuilder::new(app)
        .text("show", "显示主窗口")
        .text("hide", "隐藏到后台")
        .separator()
        .text("start_core", "启动内核")
        .text("stop_core", "停止内核")
        .text("restart_core", "重启内核")
        .separator()
        .text("mode_rule", "规则模式")
        .text("mode_global", "全局模式")
        .text("mode_direct", "直连模式")
        .separator()
        .text("quit", "退出")
        .build()?;

    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("代理客户端")
        .show_menu_on_left_click(false);

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    } else if let Ok(icon) = Image::from_bytes(include_bytes!("../icons/icon.ico")) {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

fn handle_tray_menu(app: &AppHandle, id: &str) {
    let app_handle = app.clone();
    match id {
        "show" => {
            if let Err(error) = show_main_window(&app_handle) {
                eprintln!("[tray] show failed: {error}");
            }
        }
        "hide" => {
            if let Err(error) = hide_main_window(&app_handle) {
                eprintln!("[tray] hide failed: {error}");
            }
        }
        "start_core" => spawn_core_action(app_handle, start_core_inner),
        "stop_core" => spawn_core_action(app_handle, stop_core_inner),
        "restart_core" => spawn_core_action(app_handle, restart_core_inner),
        "mode_rule" => set_mode_from_tray(app_handle, models::ProxyMode::Rule),
        "mode_global" => set_mode_from_tray(app_handle, models::ProxyMode::Global),
        "mode_direct" => set_mode_from_tray(app_handle, models::ProxyMode::Direct),
        "quit" => {
            let state = app_handle.state::<SharedState>();
            state.quit_requested.store(true, Ordering::SeqCst);
            if let Err(error) = singbox::cleanup_runtime_on_exit(&app_handle) {
                eprintln!("[tray] shutdown cleanup failed: {error}");
            }
            app_handle.exit(0);
        }
        _ => {}
    }
}

fn spawn_core_action<F, Fut>(app: AppHandle, action: F)
where
    F: FnOnce(AppHandle, SharedState) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<AppStatus, String>> + Send + 'static,
{
    let state = app.state::<SharedState>().inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = action(app, state).await {
            eprintln!("[tray] core action failed: {error}");
        }
    });
}

fn set_mode_from_tray(app: AppHandle, mode: models::ProxyMode) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<SharedState>().inner().clone();
        let was_running = {
            let core = state.core.lock().await;
            core.is_running()
        };

        match config::load_or_create_settings(&app) {
            Ok(mut settings) => {
                settings.mode = mode;
                if let Err(error) = config::save_settings(&app, &settings)
                    .and_then(|_| config::write_singbox_config(&app, &settings).map(|_| ()))
                {
                    eprintln!("[tray] mode save failed: {error}");
                    return;
                }
                if was_running {
                    let _ = restart_core_inner(app, state).await;
                }
            }
            Err(error) => eprintln!("[tray] load settings failed: {error}"),
        }
    });
}

fn spawn_auto_start_proxy(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let settings = match config::load_or_create_settings(&app) {
            Ok(settings) => settings,
            Err(error) => {
                eprintln!("[auto-start] failed to load settings: {error}");
                return;
            }
        };
        if !settings.auto_start_proxy {
            return;
        }

        let state = app.state::<SharedState>().inner().clone();
        if let Err(error) = start_core_inner(app, state).await {
            eprintln!("[auto-start] failed to start core: {error}");
        }
    });
}

fn spawn_start_hidden(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        if let Err(error) = hide_main_window(&app) {
            eprintln!("[startup-hide] failed to hide window: {error}");
        }
    });
}

fn normalize_settings_for_save(mut settings: AppSettings) -> AppSettings {
    let defaults = AppSettings::default();

    if settings.local_mixed_port == 0 {
        settings.local_mixed_port = defaults.local_mixed_port;
    }
    if settings.clash_api_port == 0 {
        settings.clash_api_port = defaults.clash_api_port;
    }
    if settings.clash_api_secret.trim().is_empty() {
        settings.clash_api_secret = defaults.clash_api_secret;
    } else {
        settings.clash_api_secret = settings.clash_api_secret.trim().to_owned();
    }

    settings.tun_interface_name = if settings.tun_interface_name.trim().is_empty() {
        defaults.tun_interface_name
    } else {
        settings.tun_interface_name.trim().to_owned()
    };
    if settings.tun_mtu < 576 {
        settings.tun_mtu = defaults.tun_mtu;
    }
    if settings.fake_ip_v4_range.trim().is_empty() {
        settings.fake_ip_v4_range = defaults.fake_ip_v4_range;
    } else {
        settings.fake_ip_v4_range = settings.fake_ip_v4_range.trim().to_owned();
    }
    if settings.fake_ip_v6_range.trim().is_empty() {
        settings.fake_ip_v6_range = defaults.fake_ip_v6_range;
    } else {
        settings.fake_ip_v6_range = settings.fake_ip_v6_range.trim().to_owned();
    }

    settings.custom_dns_servers = settings
        .custom_dns_servers
        .into_iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect();
    settings.tun_route_exclude_address = settings
        .tun_route_exclude_address
        .into_iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect();
    if settings.tun_route_exclude_address.is_empty() {
        settings.tun_route_exclude_address = defaults.tun_route_exclude_address;
    }

    settings.converter_url = settings
        .converter_url
        .take()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());

    settings
}

fn to_err(error: anyhow::Error) -> String {
    error.to_string()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(Arc::new(AppState {
            core: Mutex::new(SingboxManager::default()),
            quit_requested: AtomicBool::new(false),
        }))
        .setup(|app| {
            system::ensure_admin_on_startup(app.handle())?;
            singbox::cleanup_existing_sidecar(app.handle())?;
            system::apply_pending_elevation_settings(app.handle())?;
            let _ = recover_from_unclean_shutdown(app.handle());
            let settings = config::load_or_create_settings(app.handle())?;
            apply_auto_launch_setting(app.handle(), settings.auto_launch)?;
            config::write_singbox_config(app.handle(), &settings)?;
            setup_tray(app)?;
            if settings.start_hidden {
                spawn_start_hidden(app.handle().clone());
            }
            if settings.auto_start_proxy || settings.proxy_enabled {
                spawn_auto_start_proxy(app.handle().clone());
            }
            Ok(())
        })
        .on_menu_event(|app, event| handle_tray_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|app, event| match event {
            TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            }
            | TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } => {
                if let Err(error) = show_main_window(app) {
                    eprintln!("[tray] show failed: {error}");
                }
            }
            _ => {}
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let state = app.state::<SharedState>();
                if state.quit_requested.load(Ordering::SeqCst) {
                    if let Err(error) = singbox::cleanup_runtime_on_exit(app) {
                        eprintln!("[shutdown-cleanup] {error}");
                    }
                    return;
                }

                match config::load_or_create_settings(app) {
                    Ok(settings) if settings.hide_to_tray => {
                        api.prevent_close();
                        if let Err(error) = window.hide() {
                            eprintln!("[window] hide failed: {error}");
                        }
                    }
                    Ok(_) => {
                        state.quit_requested.store(true, Ordering::SeqCst);
                        if let Err(error) = singbox::cleanup_runtime_on_exit(app) {
                            eprintln!("[shutdown-cleanup] {error}");
                        }
                    }
                    Err(error) => {
                        eprintln!("[window] failed to load close behavior: {error}");
                        api.prevent_close();
                        if let Err(error) = window.hide() {
                            eprintln!("[window] fallback hide failed: {error}");
                        }
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            app_status,
            start_core,
            stop_core,
            restart_core,
            list_proxies,
            select_proxy,
            delay_proxy,
            check_admin,
            import_subscription,
            refresh_subscription,
            delete_subscription,
            maintenance_info,
            open_app_data_dir,
            open_log_dir,
            clear_singbox_log,
            clear_runtime_marker,
            reset_network_state,
            validate_current_config,
            export_diagnostics,
            enter_background_mode,
            restart_as_admin
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tauri application");
}
