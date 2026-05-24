use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use url::Url;

use crate::models::{AppSettings, FallbackPolicy, ProxyMode};

const SETTINGS_FILE: &str = "app_settings.json";
const SINGBOX_CONFIG_FILE: &str = "config.json";
const CORE_RUNTIME_MARKER_FILE: &str = "core_runtime.marker";
const SINGBOX_LOG_FILE: &str = "sing-box.log";

const TAG_PROXY: &str = "PROXY";
const TAG_DIRECT: &str = "DIRECT";
const TAG_BLOCK: &str = "BLOCK";
const DNS_DIRECT: &str = "dns_direct";
const DNS_REMOTE: &str = "dns_remote";
const DNS_RESOLVER: &str = "dns_resolver";
const DNS_FAKEIP: &str = "dns_fakeip";

const DEFAULT_TUN_IPV4: &str = "172.19.0.1/30";
const DEFAULT_TUN_IPV6: &str = "fdfe:dcba:9876::1/126";

const RS_GEOSITE_CN: &str = "geosite-cn";
const RS_GEOSITE_GEOLOCATION_NOT_CN: &str = "geosite-geolocation-!cn";
const RS_GEOSITE_PRIVATE: &str = "geosite-private";
const RS_GEOIP_CN: &str = "geoip-cn";

const PRIVATE_IP_CIDRS: &[&str] = &[
    "10.0.0.0/8",
    "100.64.0.0/10",
    "127.0.0.0/8",
    "169.254.0.0/16",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "::1/128",
    "fc00::/7",
    "fe80::/10",
];

pub fn app_data_dir(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .context("failed to resolve app data directory")?;
    fs::create_dir_all(&dir).context("failed to create app data directory")?;
    fs::create_dir_all(dir.join("subscriptions")).context("failed to create subscriptions dir")?;
    fs::create_dir_all(dir.join("logs")).context("failed to create logs dir")?;
    Ok(dir)
}

pub fn settings_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(app_data_dir(app)?.join(SETTINGS_FILE))
}

pub fn singbox_config_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(app_data_dir(app)?.join(SINGBOX_CONFIG_FILE))
}

pub fn singbox_log_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(app_data_dir(app)?.join("logs").join(SINGBOX_LOG_FILE))
}

pub fn core_runtime_marker_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(app_data_dir(app)?.join(CORE_RUNTIME_MARKER_FILE))
}

pub fn subscriptions_dir(app: &AppHandle) -> Result<PathBuf> {
    let dir = app_data_dir(app)?.join("subscriptions");
    fs::create_dir_all(&dir).context("failed to create subscriptions dir")?;
    Ok(dir)
}

pub fn load_or_create_settings(app: &AppHandle) -> Result<AppSettings> {
    let path = settings_path(app)?;
    if !path.exists() {
        let settings = AppSettings::default();
        save_settings(app, &settings)?;
        return Ok(settings);
    }

    let content = fs::read_to_string(&path).context("failed to read app settings")?;
    let mut settings: AppSettings =
        serde_json::from_str(&content).context("failed to parse app settings")?;
    if sanitize_subscription_metadata(&mut settings) {
        save_settings(app, &settings)?;
    }
    Ok(settings)
}

pub fn save_settings(app: &AppHandle, settings: &AppSettings) -> Result<()> {
    let path = settings_path(app)?;
    let content = serde_json::to_string_pretty(settings).context("failed to serialize settings")?;
    fs::write(path, content).context("failed to write app settings")
}

pub fn write_singbox_config(app: &AppHandle, settings: &AppSettings) -> Result<PathBuf> {
    let path = singbox_config_path(app)?;
    let config = build_singbox_config(app, settings)?;
    let content =
        serde_json::to_string_pretty(&config).context("failed to serialize sing-box config")?;
    fs::write(&path, content).context("failed to write sing-box config")?;
    Ok(path)
}

pub fn mark_core_runtime_state(app: &AppHandle, running: bool) -> Result<()> {
    let marker = core_runtime_marker_path(app)?;
    if running {
        fs::write(&marker, b"running").context("failed to write core runtime marker")?;
    } else if marker.exists() {
        fs::remove_file(&marker).context("failed to remove core runtime marker")?;
    }
    Ok(())
}

pub fn has_unclean_runtime_marker(app: &AppHandle) -> Result<bool> {
    Ok(core_runtime_marker_path(app)?.exists())
}

fn build_singbox_config(app: &AppHandle, settings: &AppSettings) -> Result<Value> {
    let mixed_listen = if settings.allow_lan {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };

    let strategy = if settings.ipv6_enabled {
        "prefer_ipv6"
    } else {
        "ipv4_only"
    };
    let remote_dns_final = match settings.mode {
        ProxyMode::Direct => DNS_DIRECT,
        ProxyMode::Global => DNS_REMOTE,
        ProxyMode::Rule => DNS_REMOTE,
    };
    let route_final = match settings.mode {
        ProxyMode::Direct => TAG_DIRECT,
        ProxyMode::Global => TAG_PROXY,
        ProxyMode::Rule => match settings.fallback {
            FallbackPolicy::Proxy => TAG_PROXY,
            FallbackPolicy::Direct => TAG_DIRECT,
        },
    };

    let (mut imported_outbounds, imported_tags) = load_imported_outbounds(app, settings)?;
    let mut selector_tags = imported_tags;
    if !selector_tags.iter().any(|tag| tag == TAG_DIRECT) {
        selector_tags.push(TAG_DIRECT.to_owned());
    }

    let mut outbounds = vec![
        json!({
            "type": "selector",
            "tag": TAG_PROXY,
            "outbounds": selector_tags,
            "interrupt_exist_connections": true
        }),
        json!({
            "type": "direct",
            "tag": TAG_DIRECT
        }),
        json!({
            "type": "block",
            "tag": TAG_BLOCK
        }),
    ];
    outbounds.append(&mut imported_outbounds);

    let mut inbounds = vec![json!({
        "type": "mixed",
        "tag": "mixed-in",
        "listen": mixed_listen,
        "listen_port": settings.local_mixed_port,
        "set_system_proxy": !settings.tun_enabled
    })];
    if settings.tun_enabled {
        inbounds.push(build_tun_inbound(settings));
    }

    let dns_servers = build_dns_servers(settings, strategy)?;
    let dns_rules = build_dns_rules(settings);
    let route_rules = build_route_rules(settings);
    let fake_dns_enabled = settings.tun_enabled && settings.fake_dns_enabled;

    Ok(json!({
        "log": {
            "level": "warn",
            "timestamp": true,
            "output": app_data_dir(app)?.join("logs").join("sing-box.log").to_string_lossy().to_string()
        },
        "experimental": {
            "cache_file": {
                "enabled": true,
                "store_rdrc": fake_dns_enabled
            },
            "clash_api": {
                "external_controller": format!("127.0.0.1:{}", settings.clash_api_port),
                "secret": settings.clash_api_secret,
                "default_mode": match settings.mode {
                    ProxyMode::Direct => "direct",
                    ProxyMode::Global => "global",
                    ProxyMode::Rule => "rule",
                }
            }
        },
        "dns": {
            "servers": dns_servers,
            "rules": dns_rules,
            "strategy": strategy,
            "independent_cache": true,
            "reverse_mapping": fake_dns_enabled,
            "final": remote_dns_final
        },
        "inbounds": inbounds,
        "outbounds": outbounds,
        "route": {
            "rule_set": build_rule_sets(),
            "rules": route_rules,
            "final": route_final,
            "auto_detect_interface": true,
            "default_domain_resolver": {
                "server": DNS_RESOLVER,
                "strategy": strategy
            }
        }
    }))
}

fn build_dns_servers(settings: &AppSettings, strategy: &str) -> Result<Vec<Value>> {
    let fallback_direct = "223.5.5.5";
    let fallback_remote = "https://1.1.1.1/dns-query";
    let direct_address = settings
        .custom_dns_servers
        .first()
        .map(String::as_str)
        .unwrap_or(fallback_direct);
    let remote_address = settings
        .custom_dns_servers
        .get(1)
        .or_else(|| settings.custom_dns_servers.first())
        .map(String::as_str)
        .unwrap_or(fallback_remote);

    let mut servers = vec![
        json!({
            "tag": DNS_RESOLVER,
            "type": "local"
        }),
        build_dns_server(
            DNS_DIRECT,
            direct_address,
            strategy,
            Some(TAG_DIRECT),
            Some(DNS_RESOLVER),
        )?,
        build_dns_server(
            DNS_REMOTE,
            remote_address,
            strategy,
            Some(TAG_PROXY),
            Some(DNS_RESOLVER),
        )?,
    ];

    if settings.tun_enabled && settings.fake_dns_enabled {
        servers.push(json!({
            "tag": DNS_FAKEIP,
            "type": "fakeip",
            "inet4_range": settings.fake_ip_v4_range,
            "inet6_range": settings.fake_ip_v6_range
        }));
    }

    Ok(servers)
}

fn build_dns_rules(settings: &AppSettings) -> Vec<Value> {
    let mut rules = vec![
        json!({ "clash_mode": "direct", "server": DNS_DIRECT }),
        json!({ "clash_mode": "global", "server": DNS_REMOTE }),
        json!({ "domain_suffix": ["lan", "local", "home.arpa"], "server": DNS_DIRECT }),
        json!({ "rule_set": RS_GEOSITE_PRIVATE, "server": DNS_DIRECT }),
        json!({ "rule_set": [RS_GEOSITE_CN, RS_GEOIP_CN], "server": DNS_DIRECT }),
        json!({ "rule_set": RS_GEOSITE_GEOLOCATION_NOT_CN, "server": DNS_REMOTE }),
    ];

    if settings.tun_enabled && settings.fake_dns_enabled {
        match settings.mode {
            ProxyMode::Global => {
                rules.push(json!({
                    "query_type": ["A", "AAAA"],
                    "server": DNS_FAKEIP
                }));
            }
            ProxyMode::Rule => {
                let insert_idx = rules
                    .iter()
                    .position(|rule| {
                        rule.get("rule_set").and_then(Value::as_str)
                            == Some(RS_GEOSITE_GEOLOCATION_NOT_CN)
                    })
                    .unwrap_or(rules.len());
                rules.insert(
                    insert_idx,
                    json!({
                        "query_type": ["A", "AAAA"],
                        "rule_set": RS_GEOSITE_GEOLOCATION_NOT_CN,
                        "server": DNS_FAKEIP
                    }),
                );
            }
            ProxyMode::Direct => {}
        }
    }

    rules
}

fn build_route_rules(settings: &AppSettings) -> Vec<Value> {
    let mut rules = vec![json!({ "action": "sniff" })];

    if settings.tun_enabled && settings.dns_guard_enabled {
        rules.push(json!({ "protocol": "dns", "action": "hijack-dns" }));
    }

    rules.extend([
        json!({ "clash_mode": "direct", "outbound": TAG_DIRECT }),
        json!({ "clash_mode": "global", "outbound": TAG_PROXY }),
        json!({ "ip_cidr": PRIVATE_IP_CIDRS, "outbound": TAG_DIRECT }),
        json!({ "domain_suffix": ["lan", "local", "home.arpa"], "outbound": TAG_DIRECT }),
        json!({ "rule_set": RS_GEOSITE_PRIVATE, "outbound": TAG_DIRECT }),
        json!({ "rule_set": [RS_GEOSITE_CN, RS_GEOIP_CN], "outbound": TAG_DIRECT }),
        json!({ "rule_set": RS_GEOSITE_GEOLOCATION_NOT_CN, "outbound": TAG_PROXY }),
    ]);

    if settings.tun_enabled && settings.fake_dns_enabled {
        rules.push(json!({
            "ip_cidr": [settings.fake_ip_v4_range.clone(), settings.fake_ip_v6_range.clone()],
            "outbound": TAG_DIRECT
        }));
    }

    rules
}

fn build_rule_sets() -> Vec<Value> {
    vec![
        remote_rule_set(
            RS_GEOSITE_PRIVATE,
            "https://gh-proxy.com/https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-private.srs",
            TAG_DIRECT,
            "7d",
        ),
        remote_rule_set(
            RS_GEOSITE_CN,
            "https://gh-proxy.com/https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-cn.srs",
            TAG_DIRECT,
            "1d",
        ),
        remote_rule_set(
            RS_GEOSITE_GEOLOCATION_NOT_CN,
            "https://gh-proxy.com/https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-geolocation-!cn.srs",
            TAG_DIRECT,
            "1d",
        ),
        remote_rule_set(
            RS_GEOIP_CN,
            "https://gh-proxy.com/https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/geoip-cn.srs",
            TAG_DIRECT,
            "1d",
        ),
    ]
}

fn remote_rule_set(tag: &str, url: &str, download_detour: &str, update_interval: &str) -> Value {
    json!({
        "tag": tag,
        "type": "remote",
        "format": "binary",
        "url": url,
        "download_detour": download_detour,
        "update_interval": update_interval
    })
}

fn build_tun_inbound(settings: &AppSettings) -> Value {
    let mut addresses = vec![DEFAULT_TUN_IPV4.to_owned()];
    if settings.ipv6_enabled {
        addresses.push(DEFAULT_TUN_IPV6.to_owned());
    }

    json!({
        "type": "tun",
        "tag": "tun-in",
        "interface_name": settings.tun_interface_name,
        "address": addresses,
        "auto_route": settings.tun_auto_route,
        "strict_route": settings.tun_strict_route,
        "stack": "mixed",
        "mtu": settings.tun_mtu,
        "route_exclude_address": settings.tun_route_exclude_address
    })
}

fn build_dns_server(
    tag: &str,
    raw_address: &str,
    strategy: &str,
    detour: Option<&str>,
    resolver_tag: Option<&str>,
) -> Result<Value> {
    let raw = raw_address.trim();
    if raw.is_empty() {
        anyhow::bail!("empty dns address for {}", tag);
    }

    if raw.eq_ignore_ascii_case("local") {
        return Ok(json!({
            "tag": tag,
            "type": "local"
        }));
    }

    let mut server = json!({
        "tag": tag
    });

    if raw.contains("://") {
        let url = Url::parse(raw).with_context(|| format!("invalid dns url: {}", raw))?;
        let server_type = match url.scheme() {
            "https" => "https",
            "h3" => "h3",
            "quic" => "quic",
            "tls" => "tls",
            "tcp" => "tcp",
            "udp" => "udp",
            other => anyhow::bail!("unsupported dns scheme {} for {}", other, raw),
        };
        let host = url
            .host_str()
            .with_context(|| format!("dns server missing host: {}", raw))?;

        server["type"] = server_type.into();
        server["server"] = host.into();
        server["server_port"] = url.port().unwrap_or(default_dns_port(server_type)).into();
        if matches!(server_type, "https" | "h3") {
            server["path"] = url
                .path()
                .trim()
                .is_empty()
                .then_some("/dns-query")
                .unwrap_or(url.path())
                .into();
        }
    } else {
        let (host, port) = parse_host_port(raw)?;
        server["type"] = "udp".into();
        server["server"] = host.into();
        server["server_port"] = port.unwrap_or(53).into();
    }

    server["domain_resolver"] = json!({
        "server": resolver_tag.unwrap_or(DNS_RESOLVER),
        "strategy": strategy
    });

    if let Some(detour) =
        detour.filter(|value| !value.trim().is_empty() && !value.eq_ignore_ascii_case(TAG_DIRECT))
    {
        server["detour"] = detour.into();
    }

    Ok(server)
}

fn parse_host_port(raw: &str) -> Result<(String, Option<u16>)> {
    if raw.starts_with('[') {
        let end = raw
            .find(']')
            .with_context(|| format!("invalid dns address: {}", raw))?;
        let host = raw[1..end].trim();
        if host.is_empty() {
            anyhow::bail!("invalid dns address: {}", raw);
        }
        let port = if end + 1 < raw.len() {
            raw[end + 1..]
                .strip_prefix(':')
                .with_context(|| format!("invalid dns address: {}", raw))?
                .parse::<u16>()
                .with_context(|| format!("invalid dns port: {}", raw))?
                .into()
        } else {
            None
        };
        return Ok((host.to_owned(), port));
    }

    if raw.matches(':').count() > 1 {
        return Ok((raw.to_owned(), None));
    }

    if let Some((host, port)) = raw.rsplit_once(':') {
        if !host.is_empty() && port.chars().all(|char| char.is_ascii_digit()) {
            let parsed = port
                .parse::<u16>()
                .with_context(|| format!("invalid dns port: {}", raw))?;
            return Ok((host.to_owned(), Some(parsed)));
        }
    }

    Ok((raw.to_owned(), None))
}

fn default_dns_port(server_type: &str) -> u16 {
    match server_type {
        "https" | "h3" | "tls" | "quic" => 443,
        _ => 53,
    }
}

fn load_imported_outbounds(
    app: &AppHandle,
    settings: &AppSettings,
) -> Result<(Vec<Value>, Vec<String>)> {
    let dir = subscriptions_dir(app)?;
    let mut outbounds = Vec::new();
    let mut tags = Vec::new();

    for subscription in &settings.subscriptions {
        if !subscription.enabled {
            continue;
        }
        let path = dir.join(format!("{}.json", subscription.id));
        if !path.exists() {
            continue;
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read subscription cache {}", path.display()))?;
        let mut value = serde_json::from_str::<Value>(&content)
            .with_context(|| format!("failed to parse subscription cache {}", path.display()))?;
        let Some(items) = value.get("outbounds").and_then(Value::as_array).cloned() else {
            continue;
        };

        let mut clean_items = Vec::with_capacity(items.len());
        let mut removed_items = false;
        for item in items {
            let tag = item.get("tag").and_then(Value::as_str).unwrap_or_default();
            let outbound_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
            if tag.is_empty()
                || is_reserved_outbound(tag, outbound_type)
                || is_informational_tag(tag)
            {
                removed_items = true;
                continue;
            }
            clean_items.push(item.clone());
            if !tags.iter().any(|existing| existing == tag) {
                tags.push(tag.to_owned());
                outbounds.push(normalize_imported_outbound(item));
            }
        }

        if removed_items {
            value["outbounds"] = Value::Array(clean_items);
            fs::write(
                &path,
                serde_json::to_string_pretty(&value)
                    .context("failed to serialize sanitized subscription cache")?,
            )
            .with_context(|| format!("failed to write subscription cache {}", path.display()))?;
        }
    }

    Ok((outbounds, tags))
}

fn normalize_imported_outbound(mut outbound: Value) -> Value {
    if let Some(flow) = outbound.get("flow").and_then(Value::as_str) {
        outbound["flow"] = normalize_flow_value(flow).into();
    }

    let uses_reality = outbound
        .get("tls")
        .and_then(|tls| tls.get("reality"))
        .and_then(|reality| reality.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !uses_reality {
        return outbound;
    }

    let fingerprint = outbound
        .get("tls")
        .and_then(|tls| tls.get("utls"))
        .and_then(|utls| utls.get("fingerprint"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("chrome");

    outbound["tls"]["utls"] = json!({
        "enabled": true,
        "fingerprint": fingerprint
    });
    outbound
}

fn normalize_flow_value(flow: &str) -> String {
    match flow.trim() {
        "xtls-rprx-vision-udp443" => "xtls-rprx-vision".to_owned(),
        other => other.to_owned(),
    }
}

fn is_reserved_outbound(tag: &str, outbound_type: &str) -> bool {
    matches!(
        tag,
        TAG_DIRECT | TAG_PROXY | "GLOBAL" | "REJECT" | "dns-out" | TAG_BLOCK
    ) || matches!(outbound_type, "direct" | "block" | "dns")
}

fn sanitize_subscription_metadata(settings: &mut AppSettings) -> bool {
    let mut changed = false;

    for subscription in &mut settings.subscriptions {
        let had_tags = !subscription.tags.is_empty();
        let before = subscription.tags.len();
        subscription
            .tags
            .retain(|tag| !is_reserved_subscription_tag(tag) && !is_informational_tag(tag));

        if subscription.tags.len() != before {
            changed = true;
        }

        if had_tags && subscription.node_count != subscription.tags.len() {
            subscription.node_count = subscription.tags.len();
            changed = true;
        }
    }

    changed
}

fn is_reserved_subscription_tag(tag: &str) -> bool {
    matches!(
        tag.trim(),
        TAG_DIRECT | TAG_PROXY | "GLOBAL" | "REJECT" | "dns-out" | TAG_BLOCK
    )
}

fn is_informational_tag(tag: &str) -> bool {
    is_informational_tag_clean(tag)
}

fn is_informational_tag_clean(tag: &str) -> bool {
    let normalized = tag.trim().to_lowercase();
    if normalized.is_empty() {
        return true;
    }

    let keywords = [
        "\u{5269}\u{4f59}\u{6d41}\u{91cf}",
        "\u{8ddd}\u{79bb}\u{4e0b}\u{6b21}\u{91cd}\u{7f6e}\u{5269}\u{4f59}",
        "\u{5957}\u{9910}\u{5230}\u{671f}",
        "\u{5b98}\u{7f51}",
        "\u{5907}\u{7528}\u{7f51}\u{5740}",
        "\u{8df3}\u{8f6c}\u{57df}\u{540d}",
        "\u{8bf7}\u{52ff}\u{8fde}\u{63a5}",
        "\u{5ba2}\u{670d}",
        "\u{66f4}\u{65b0}\u{5730}\u{5740}",
        "\u{5230}\u{671f}\u{65f6}\u{95f4}",
        "\u{8fc7}\u{671f}\u{65f6}\u{95f4}",
        "\u{6d41}\u{91cf}",
        "\u{6709}\u{6548}\u{671f}",
        "\u{8054}\u{7cfb}",
        "\u{7535}\u{62a5}\u{7fa4}",
        "\u{7fa4}\u{7ec4}",
        "\u{516c}\u{544a}",
        "\u{63d0}\u{793a}",
        "\u{8bf4}\u{660e}",
        "\u{8ba2}\u{9605}",
        "\u{83b7}\u{53d6}\u{66f4}\u{591a}",
        "\u{8d2d}\u{4e70}",
        "\u{7eed}\u{8d39}",
        "\u{7f51}\u{5740}",
        "telegram",
        "http://",
        "https://",
    ];

    keywords
        .iter()
        .any(|keyword| normalized.starts_with(keyword))
        || normalized.contains("\u{8bf7}\u{52ff}\u{8fde}\u{63a5}")
        || normalized.contains('{')
        || normalized.contains('}')
}

#[cfg(test)]
mod tests {
    use super::{
        build_dns_rules, build_dns_server, build_tun_inbound, DNS_FAKEIP, DNS_RESOLVER, TAG_DIRECT,
    };
    use crate::models::{AppSettings, ProxyMode};

    #[test]
    fn tun_inbound_contains_ipv6_when_enabled() {
        let settings = AppSettings {
            tun_enabled: true,
            ipv6_enabled: true,
            ..AppSettings::default()
        };
        let tun = build_tun_inbound(&settings);
        let addresses = tun["address"].as_array().expect("tun addresses");
        assert_eq!(addresses.len(), 2);
    }

    #[test]
    fn dns_builder_supports_https_servers() {
        let server = build_dns_server(
            "dns_remote",
            "https://1.1.1.1/dns-query",
            "ipv4_only",
            Some("PROXY"),
            Some(DNS_RESOLVER),
        )
        .expect("dns server should build");

        assert_eq!(server["type"], "https");
        assert_eq!(server["server"], "1.1.1.1");
        assert_eq!(server["path"], "/dns-query");
    }

    #[test]
    fn dns_builder_omits_direct_detour() {
        let server = build_dns_server(
            "dns_direct",
            "223.5.5.5",
            "ipv4_only",
            Some(TAG_DIRECT),
            Some(DNS_RESOLVER),
        )
        .expect("dns server should build");

        assert!(server.get("detour").is_none());
    }

    #[test]
    fn mixed_inbound_does_not_use_legacy_sniff_fields() {
        let settings = AppSettings::default();
        let mixed = serde_json::json!({
            "type": "mixed",
            "tag": "mixed-in",
            "listen": "127.0.0.1",
            "listen_port": settings.local_mixed_port
        });

        assert!(mixed.get("sniff").is_none());
        assert!(mixed.get("sniff_override_destination").is_none());
    }

    #[test]
    fn mixed_inbound_enables_system_proxy_when_tun_is_disabled() {
        let settings = AppSettings::default();
        let mixed = serde_json::json!({
            "type": "mixed",
            "tag": "mixed-in",
            "listen": "127.0.0.1",
            "listen_port": settings.local_mixed_port,
            "set_system_proxy": !settings.tun_enabled
        });

        assert_eq!(mixed["set_system_proxy"], true);
    }

    #[test]
    fn tun_rule_mode_inserts_fakeip_dns_rule_for_non_cn_queries() {
        let settings = AppSettings {
            tun_enabled: true,
            mode: ProxyMode::Rule,
            ..AppSettings::default()
        };
        let rules = build_dns_rules(&settings);
        assert!(rules.iter().any(|rule| {
            rule.get("server").and_then(serde_json::Value::as_str) == Some(DNS_FAKEIP)
        }));
    }

    #[test]
    fn tun_rule_mode_keeps_cn_dns_direct_before_non_cn_fakeip() {
        let settings = AppSettings {
            tun_enabled: true,
            mode: ProxyMode::Rule,
            dns_guard_enabled: true,
            ..AppSettings::default()
        };
        let rules = build_dns_rules(&settings);
        let cn_idx = rules
            .iter()
            .position(|rule| {
                rule.get("server").and_then(serde_json::Value::as_str) == Some("dns_direct")
            })
            .expect("cn direct dns rule");
        let fakeip_idx = rules
            .iter()
            .position(|rule| {
                rule.get("server").and_then(serde_json::Value::as_str) == Some(DNS_FAKEIP)
            })
            .expect("non-cn fakeip dns rule");

        assert!(cn_idx < fakeip_idx);
        assert!(!rules.iter().any(|rule| rule.get("inbound").is_some()));
    }

    #[test]
    fn tun_global_mode_routes_all_a_aaaa_queries_to_fakeip() {
        let settings = AppSettings {
            tun_enabled: true,
            mode: ProxyMode::Global,
            ..AppSettings::default()
        };
        let rules = build_dns_rules(&settings);
        assert!(rules.iter().any(|rule| {
            rule.get("server").and_then(serde_json::Value::as_str) == Some(DNS_FAKEIP)
                && rule.get("query_type").is_some()
                && rule.get("rule_set").is_none()
        }));
    }

    #[test]
    fn rule_mode_keeps_direct_fallback_available() {
        let settings = AppSettings {
            mode: ProxyMode::Rule,
            ..AppSettings::default()
        };
        assert!(build_tun_inbound(&settings).get("type").is_some());
    }

    #[test]
    fn subscription_metadata_drops_informational_tags() {
        let mut settings = AppSettings::default();
        settings
            .subscriptions
            .push(crate::models::SubscriptionInfo {
                id: "sub".to_owned(),
                name: "sub".to_owned(),
                url: "manual://import/test".to_owned(),
                enabled: true,
                tags: vec![
                    "\u{5269}\u{4f59}\u{6d41}\u{91cf}: 10GB".to_owned(),
                    "https://example.com".to_owned(),
                    "HK-01".to_owned(),
                ],
                node_count: 3,
                updated_at: 0,
                status: crate::models::SubscriptionStatus::Active,
                message: None,
            });

        assert!(super::sanitize_subscription_metadata(&mut settings));
        assert_eq!(settings.subscriptions[0].tags, vec!["HK-01"]);
        assert_eq!(settings.subscriptions[0].node_count, 1);
    }
}
