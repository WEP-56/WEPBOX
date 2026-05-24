use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub local_mixed_port: u16,
    pub clash_api_port: u16,
    pub clash_api_secret: String,
    pub tun_enabled: bool,
    pub fake_dns_enabled: bool,
    pub proxy_enabled: bool,
    pub mode: ProxyMode,
    pub fallback: FallbackPolicy,
    pub auto_update_hours: u16,
    pub follow_system_theme: bool,
    pub notify_on_failure: bool,
    pub auto_launch: bool,
    pub auto_start_proxy: bool,
    pub start_hidden: bool,
    pub hide_to_tray: bool,
    pub auto_select_fastest: bool,
    pub auto_switch_on_failure: bool,
    pub speed_test_interval: SpeedTestInterval,
    pub dns_guard_enabled: bool,
    pub ipv6_enabled: bool,
    pub udp_acceleration_enabled: bool,
    pub allow_lan: bool,
    pub experimental_quic: bool,
    pub custom_dns_servers: Vec<String>,
    pub fake_ip_v4_range: String,
    pub fake_ip_v6_range: String,
    pub tun_interface_name: String,
    pub tun_mtu: u16,
    pub tun_auto_route: bool,
    pub tun_strict_route: bool,
    pub tun_route_exclude_address: Vec<String>,
    pub converter_url: Option<String>,
    pub resume_after_elevation: bool,
    #[serde(default)]
    pub subscriptions: Vec<SubscriptionInfo>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            local_mixed_port: 7890,
            clash_api_port: 9090,
            clash_api_secret: uuid::Uuid::new_v4().to_string(),
            tun_enabled: false,
            fake_dns_enabled: true,
            proxy_enabled: false,
            mode: ProxyMode::Rule,
            fallback: FallbackPolicy::Direct,
            auto_update_hours: 24,
            follow_system_theme: true,
            notify_on_failure: true,
            auto_launch: true,
            auto_start_proxy: true,
            start_hidden: false,
            hide_to_tray: true,
            auto_select_fastest: true,
            auto_switch_on_failure: true,
            speed_test_interval: SpeedTestInterval::Every1Hour,
            dns_guard_enabled: true,
            ipv6_enabled: true,
            udp_acceleration_enabled: true,
            allow_lan: true,
            experimental_quic: false,
            custom_dns_servers: Vec::new(),
            fake_ip_v4_range: "198.18.0.0/15".to_owned(),
            fake_ip_v6_range: "fc00::/18".to_owned(),
            tun_interface_name: "singbox_tun".to_owned(),
            tun_mtu: 1500,
            tun_auto_route: true,
            tun_strict_route: true,
            tun_route_exclude_address: vec![
                "10.0.0.0/8".to_owned(),
                "100.64.0.0/10".to_owned(),
                "127.0.0.0/8".to_owned(),
                "169.254.0.0/16".to_owned(),
                "172.16.0.0/12".to_owned(),
                "192.168.0.0/16".to_owned(),
                "::1/128".to_owned(),
                "fc00::/7".to_owned(),
                "fe80::/10".to_owned(),
            ],
            converter_url: None,
            resume_after_elevation: false,
            subscriptions: Vec::new(),
        }
    }
}

impl AppSettings {
    pub fn api_base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.clash_api_port)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProxyMode {
    Rule,
    Global,
    Direct,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FallbackPolicy {
    Direct,
    Proxy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SpeedTestInterval {
    Never,
    Every30Minutes,
    Every1Hour,
    Every24Hours,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub core_running: bool,
    pub core_healthy: bool,
    pub core_last_exit: Option<String>,
    pub core_started_at: Option<u64>,
    pub api_base_url: String,
    pub local_mixed_port: u16,
    pub tun_enabled: bool,
    pub proxy_enabled: bool,
    pub mode: ProxyMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyList {
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectProxyRequest {
    pub group: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionInfo {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    pub node_count: usize,
    pub updated_at: u64,
    pub status: SubscriptionStatus,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubscriptionStatus {
    Active,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSubscriptionRequest {
    pub url: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSubscriptionResult {
    pub subscription: SubscriptionInfo,
    pub node_count: usize,
    pub restarted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceInfo {
    pub app_data_dir: String,
    pub settings_path: String,
    pub config_path: String,
    pub log_path: String,
    pub runtime_marker_path: String,
    pub subscriptions_dir: String,
    pub sidecar_path: Option<String>,
    pub sidecar_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceActionResult {
    pub message: String,
    pub path: Option<String>,
}

fn default_true() -> bool {
    true
}
