use std::{
    collections::{HashMap, HashSet},
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose, Engine};
use serde_json::{json, Value};
use tauri::AppHandle;
use url::Url;

use crate::{
    config,
    models::{
        AppSettings, ImportSubscriptionRequest, SubscriptionInfo, SubscriptionRefreshSummary,
        SubscriptionStatus,
    },
};

pub async fn import_subscription(
    app: &AppHandle,
    request: ImportSubscriptionRequest,
) -> Result<SubscriptionInfo> {
    let source = request.url.trim();
    if source.is_empty() {
        bail!("订阅内容不能为空");
    }
    let is_remote_url = source.starts_with("http://") || source.starts_with("https://");

    let mut settings = config::load_or_create_settings(app)?;
    let text = if is_remote_url {
        fetch_text(source).await?
    } else {
        source.to_owned()
    };
    let mut converted =
        parse_or_convert_subscription(&settings, is_remote_url.then_some(source), &text).await?;
    sanitize_converted_subscription(&mut converted);
    let outbounds = converted
        .get("outbounds")
        .and_then(Value::as_array)
        .context("转换结果缺少 sing-box outbounds")?;

    let tags = collect_importable_tags(outbounds);
    if tags.is_empty() {
        bail!("没有识别到可导入的代理节点");
    }

    let source_key = subscription_source_key(source, is_remote_url);
    let existing_subscription = if is_remote_url {
        settings
            .subscriptions
            .iter()
            .find(|subscription| subscription.url == source_key)
            .cloned()
    } else {
        None
    };
    let id = existing_subscription
        .as_ref()
        .map(|subscription| subscription.id.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let name = request
        .name
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| infer_name(source, settings.subscriptions.len() + 1, is_remote_url));

    let dir = config::subscriptions_dir(app)?;
    fs::write(dir.join(format!("{id}.raw")), text).context("failed to write raw subscription")?;
    fs::write(
        dir.join(format!("{id}.json")),
        serde_json::to_string_pretty(&converted)
            .context("failed to serialize subscription cache")?,
    )
    .context("failed to write converted subscription")?;

    let node_count = tags.len();
    let subscription = SubscriptionInfo {
        id,
        name,
        url: source_key,
        enabled: existing_subscription
            .as_ref()
            .map(|subscription| subscription.enabled)
            .unwrap_or(true),
        tags,
        node_count,
        updated_at: now_unix(),
        status: SubscriptionStatus::Active,
        message: None,
    };

    upsert_subscription(&mut settings, subscription.clone());
    config::save_settings(app, &settings)?;
    config::write_singbox_config(app, &settings)?;

    Ok(subscription)
}

pub async fn refresh_subscription(app: &AppHandle, id: &str) -> Result<SubscriptionInfo> {
    let settings = config::load_or_create_settings(app)?;
    let subscription = settings
        .subscriptions
        .iter()
        .find(|item| item.id == id)
        .cloned()
        .with_context(|| format!("subscription not found: {id}"))?;

    if !subscription_is_remote_url(&subscription.url) {
        bail!("手动导入的节点不支持自动更新");
    }

    import_subscription(
        app,
        ImportSubscriptionRequest {
            url: subscription.url,
            name: Some(subscription.name),
        },
    )
    .await
}

pub async fn refresh_remote_subscriptions(
    app: &AppHandle,
    due_after_secs: Option<u64>,
) -> Result<SubscriptionRefreshSummary> {
    let settings = config::load_or_create_settings(app)?;
    let now = now_unix();
    let remote_subscriptions: Vec<SubscriptionInfo> = settings
        .subscriptions
        .iter()
        .filter(|subscription| subscription_is_remote_url(&subscription.url))
        .cloned()
        .collect();

    let mut summary = SubscriptionRefreshSummary {
        checked: remote_subscriptions.len(),
        refreshed: 0,
        failed: 0,
        skipped: 0,
        node_count: 0,
        restarted: false,
        failures: Vec::new(),
    };

    for subscription in remote_subscriptions {
        if let Some(due_after_secs) = due_after_secs {
            let age = now.saturating_sub(subscription.updated_at);
            if age < due_after_secs {
                summary.skipped += 1;
                continue;
            }
        }

        match refresh_subscription(app, &subscription.id).await {
            Ok(updated) => {
                summary.refreshed += 1;
                summary.node_count += updated.node_count;
            }
            Err(error) => {
                summary.failed += 1;
                let message = error.to_string();
                summary
                    .failures
                    .push(format!("{}: {}", subscription.name, message));
                mark_subscription_failed(app, &subscription.id, &message)?;
            }
        }
    }

    Ok(summary)
}

pub fn delete_subscription(app: &AppHandle, id: &str) -> Result<()> {
    let mut settings = config::load_or_create_settings(app)?;
    let before = settings.subscriptions.len();
    settings.subscriptions.retain(|item| item.id != id);
    if settings.subscriptions.len() == before {
        bail!("subscription not found: {id}");
    }

    let dir = config::subscriptions_dir(app)?;
    remove_if_exists(dir.join(format!("{id}.raw")))?;
    remove_if_exists(dir.join(format!("{id}.json")))?;

    config::save_settings(app, &settings)?;
    config::write_singbox_config(app, &settings)?;
    Ok(())
}

pub fn rename_subscription(app: &AppHandle, id: &str, name: &str) -> Result<SubscriptionInfo> {
    let mut settings = config::load_or_create_settings(app)?;
    let name = name.trim();
    if name.is_empty() {
        bail!("subscription name cannot be empty");
    }

    let Some(subscription) = settings
        .subscriptions
        .iter_mut()
        .find(|subscription| subscription.id == id)
    else {
        bail!("subscription not found: {id}");
    };

    subscription.name = name.to_owned();
    let updated = subscription.clone();
    config::save_settings(app, &settings)?;
    Ok(updated)
}

pub fn clear_subscription_cache(app: &AppHandle) -> Result<usize> {
    let mut settings = config::load_or_create_settings(app)?;
    let removed = settings.subscriptions.len();
    settings.subscriptions.clear();

    let dir = config::subscriptions_dir(app)?;
    if dir.exists() {
        for entry in fs::read_dir(&dir)
            .with_context(|| format!("failed to read subscriptions dir {}", dir.display()))?
        {
            let path = entry?.path();
            if matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("raw" | "json")
            ) {
                remove_if_exists(path)?;
            }
        }
    }

    config::save_settings(app, &settings)?;
    config::write_singbox_config(app, &settings)?;
    Ok(removed)
}

fn upsert_subscription(settings: &mut AppSettings, subscription: SubscriptionInfo) {
    if let Some(existing) = settings
        .subscriptions
        .iter_mut()
        .find(|item| item.id == subscription.id)
    {
        *existing = subscription;
    } else {
        settings.subscriptions.push(subscription);
    }
}

fn mark_subscription_failed(app: &AppHandle, id: &str, message: &str) -> Result<()> {
    let mut settings = config::load_or_create_settings(app)?;
    let Some(subscription) = settings
        .subscriptions
        .iter_mut()
        .find(|subscription| subscription.id == id)
    else {
        return Ok(());
    };

    subscription.status = SubscriptionStatus::Failed;
    subscription.message = Some(message.to_owned());
    subscription.updated_at = now_unix();
    config::save_settings(app, &settings)
}

async fn fetch_text(url: &str) -> Result<String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .context("failed to build http client")?
        .get(url)
        .header("User-Agent", "wepbox-proxy-client/0.1")
        .send()
        .await
        .context("订阅下载失败")?
        .error_for_status()
        .context("订阅服务器返回失败状态")?
        .text()
        .await
        .context("订阅内容读取失败")
}

async fn parse_or_convert_subscription(
    settings: &AppSettings,
    source_url: Option<&str>,
    text: &str,
) -> Result<Value> {
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        if value.get("outbounds").and_then(Value::as_array).is_some() {
            return Ok(value);
        }
    }

    if let Ok(value) = parse_clash_yaml(text) {
        return Ok(value);
    }

    if let Ok(value) = parse_v2ray_subscription(text) {
        return Ok(value);
    }

    if let (Some(converter_url), Some(source_url)) = (settings.converter_url.as_deref(), source_url)
    {
        let converted = fetch_converted(converter_url, source_url).await?;
        let value = serde_json::from_str::<Value>(&converted).context("转换服务返回的不是 JSON")?;
        if value.get("outbounds").and_then(Value::as_array).is_some() {
            return Ok(value);
        }
        bail!("转换服务返回结果缺少 sing-box outbounds");
    }

    bail!("无法识别订阅格式；当前本地支持 Clash YAML、vmess/vless/trojan/ss URI、V2Ray Base64 和 sing-box JSON")
}

async fn fetch_converted(converter_url: &str, source_url: &str) -> Result<String> {
    let base = converter_url.trim_end_matches('/');
    let url = format!(
        "{base}/sub?target=singbox&url={}",
        urlencoding::encode(source_url)
    );
    fetch_text(&url).await
}

fn collect_importable_tags(outbounds: &[Value]) -> Vec<String> {
    let mut seen = HashSet::new();
    outbounds
        .iter()
        .filter_map(|outbound| {
            let tag = outbound.get("tag")?.as_str()?.trim();
            let outbound_type = outbound.get("type")?.as_str()?.trim();
            if is_reserved_outbound(tag, outbound_type)
                || is_informational_tag(tag)
                || !is_importable_outbound(outbound, tag, outbound_type)
                || !seen.insert(tag.to_owned())
            {
                None
            } else {
                Some(tag.to_owned())
            }
        })
        .collect()
}

fn sanitize_converted_subscription(value: &mut Value) {
    let Some(outbounds) = value.get_mut("outbounds").and_then(Value::as_array_mut) else {
        return;
    };

    let mut sanitized = Vec::with_capacity(outbounds.len());
    let mut seen_tags = HashMap::<String, usize>::new();
    for mut outbound in std::mem::take(outbounds) {
        normalize_outbound_strings(&mut outbound);
        let tag = outbound
            .get("tag")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let outbound_type = outbound
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();

        if !is_importable_outbound(&outbound, tag, outbound_type) {
            continue;
        }

        let unique_tag = unique_tag(tag, &mut seen_tags);
        outbound["tag"] = unique_tag.into();
        sanitized.push(outbound);
    }
    *outbounds = sanitized;
}

fn parse_clash_yaml(text: &str) -> Result<Value> {
    let value = serde_yaml::from_str::<serde_yaml::Value>(text).context("不是 Clash YAML")?;
    let proxies = value
        .get("proxies")
        .and_then(serde_yaml::Value::as_sequence)
        .context("Clash YAML 缺少 proxies")?;

    let mut outbounds = Vec::new();
    for proxy in proxies {
        if let Some(outbound) = clash_proxy_to_singbox(proxy) {
            outbounds.push(outbound);
        }
    }

    if outbounds.is_empty() {
        bail!("Clash YAML 中没有可转换的 ss/vmess/vless/trojan 节点");
    }

    Ok(json_outbounds(outbounds))
}

fn clash_proxy_to_singbox(proxy: &serde_yaml::Value) -> Option<Value> {
    let typ = yaml_str(proxy, "type")?.to_ascii_lowercase();
    let tag = yaml_str(proxy, "name").unwrap_or("未命名节点");
    let server = yaml_str(proxy, "server")?;
    let server_port = yaml_u16(proxy, "port")?;

    let mut outbound = match typ.as_str() {
        "ss" | "shadowsocks" => json_obj([
            ("type", "shadowsocks".into()),
            ("tag", tag.into()),
            ("server", server.into()),
            ("server_port", server_port.into()),
            ("method", yaml_str(proxy, "cipher").unwrap_or("none").into()),
            (
                "password",
                yaml_str(proxy, "password").unwrap_or_default().into(),
            ),
        ]),
        "vmess" => json_obj([
            ("type", "vmess".into()),
            ("tag", tag.into()),
            ("server", server.into()),
            ("server_port", server_port.into()),
            ("uuid", yaml_str(proxy, "uuid")?.into()),
            (
                "security",
                yaml_str(proxy, "cipher").unwrap_or("auto").into(),
            ),
            ("alter_id", yaml_i64(proxy, "alterId").unwrap_or(0).into()),
        ]),
        "vless" => {
            let mut item = json_obj([
                ("type", "vless".into()),
                ("tag", tag.into()),
                ("server", server.into()),
                ("server_port", server_port.into()),
                ("uuid", yaml_str(proxy, "uuid")?.into()),
            ]);
            if let Some(flow) = yaml_str(proxy, "flow") {
                item["flow"] = normalize_flow_value(flow).into();
            }
            item
        }
        "trojan" => json_obj([
            ("type", "trojan".into()),
            ("tag", tag.into()),
            ("server", server.into()),
            ("server_port", server_port.into()),
            ("password", yaml_str(proxy, "password")?.into()),
        ]),
        "hysteria2" | "hy2" => {
            let mut item = json_obj([
                ("type", "hysteria2".into()),
                ("tag", tag.into()),
                ("server", server.into()),
                ("server_port", server_port.into()),
                ("password", yaml_str(proxy, "password")?.into()),
            ]);
            apply_clash_hysteria2_obfs(proxy, &mut item);
            item
        }
        "tuic" => {
            let mut item = json_obj([
                ("type", "tuic".into()),
                ("tag", tag.into()),
                ("server", server.into()),
                ("server_port", server_port.into()),
                ("uuid", yaml_str(proxy, "uuid")?.into()),
                ("password", yaml_str(proxy, "password")?.into()),
            ]);
            if let Some(value) = yaml_str(proxy, "congestion-controller")
                .or_else(|| yaml_str(proxy, "congestion_control"))
                .or_else(|| yaml_str(proxy, "congestion-control"))
            {
                item["congestion_control"] = value.into();
            }
            if let Some(value) =
                yaml_str(proxy, "udp-relay-mode").or_else(|| yaml_str(proxy, "udp_relay_mode"))
            {
                item["udp_relay_mode"] = value.into();
            }
            item
        }
        _ => return None,
    };

    apply_clash_tls(proxy, &mut outbound);
    apply_clash_transport(proxy, &mut outbound);
    Some(outbound)
}

fn apply_clash_tls(proxy: &serde_yaml::Value, outbound: &mut Value) {
    let tls_enabled = yaml_bool(proxy, "tls").unwrap_or(false)
        || yaml_bool(proxy, "reality-opts").unwrap_or(false)
        || yaml_str(proxy, "security")
            .map(|value| matches!(value, "tls" | "reality"))
            .unwrap_or(false)
        || matches!(
            outbound.get("type").and_then(Value::as_str),
            Some("hysteria2" | "tuic")
        );
    if !tls_enabled {
        return;
    }

    let mut tls = serde_json::Map::new();
    tls.insert("enabled".into(), true.into());
    if let Some(server_name) = yaml_str(proxy, "servername").or_else(|| yaml_str(proxy, "sni")) {
        tls.insert("server_name".into(), server_name.into());
    }
    if let Some(skip) = yaml_bool(proxy, "skip-cert-verify") {
        tls.insert("insecure".into(), skip.into());
    }
    if let Some(fp) = yaml_str(proxy, "client-fingerprint") {
        tls.insert(
            "utls".into(),
            json!({
                "enabled": true,
                "fingerprint": fp
            }),
        );
    }
    if let Some(reality) = proxy
        .get("reality-opts")
        .and_then(serde_yaml::Value::as_mapping)
    {
        let public_key = yaml_mapping_str(reality, "public-key");
        if let Some(public_key) = public_key {
            tls.insert(
                "reality".into(),
                json!({
                    "enabled": true,
                    "public_key": public_key,
                    "short_id": yaml_mapping_str(reality, "short-id").unwrap_or_default()
                }),
            );
        }
    }
    ensure_reality_utls(
        &mut tls,
        yaml_str(proxy, "client-fingerprint"),
        yaml_str(proxy, "security") == Some("reality")
            || proxy
                .get("reality-opts")
                .and_then(serde_yaml::Value::as_mapping)
                .is_some(),
    );
    outbound["tls"] = Value::Object(tls);
}

fn apply_clash_hysteria2_obfs(proxy: &serde_yaml::Value, outbound: &mut Value) {
    let obfs_type = yaml_str(proxy, "obfs")
        .or_else(|| yaml_str(proxy, "obfs-type"))
        .unwrap_or_default();
    let obfs_password = yaml_str(proxy, "obfs-password")
        .or_else(|| yaml_str(proxy, "obfs_password"))
        .unwrap_or_default();
    if !obfs_type.is_empty() || !obfs_password.is_empty() {
        outbound["obfs"] = json!({
            "type": if obfs_type.is_empty() { "salamander" } else { obfs_type },
            "password": obfs_password
        });
    }
}

fn apply_clash_transport(proxy: &serde_yaml::Value, outbound: &mut Value) {
    let network = yaml_str(proxy, "network").unwrap_or_default();
    match network {
        "ws" | "websocket" => {
            let opts = proxy.get("ws-opts");
            let path = opts
                .and_then(|v| v.get("path"))
                .and_then(serde_yaml::Value::as_str)
                .unwrap_or("/");
            let host = opts
                .and_then(|v| v.get("headers"))
                .and_then(|v| v.get("Host"))
                .and_then(serde_yaml::Value::as_str);
            let mut transport = json!({
                "type": "ws",
                "path": path
            });
            if let Some(host) = host {
                transport["headers"] = json!({ "Host": host });
            }
            outbound["transport"] = transport;
        }
        "grpc" => {
            let service_name = proxy
                .get("grpc-opts")
                .and_then(|v| v.get("grpc-service-name"))
                .and_then(serde_yaml::Value::as_str)
                .unwrap_or_default();
            outbound["transport"] = json!({
                "type": "grpc",
                "service_name": service_name
            });
        }
        _ => {}
    }
}

fn parse_v2ray_subscription(text: &str) -> Result<Value> {
    let normalized = decode_base64_text(text).unwrap_or_else(|| text.to_owned());
    let mut outbounds = Vec::new();

    for raw_line in normalized.lines() {
        let line = normalize_proxy_line(raw_line);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(outbound) = parse_proxy_uri(&line) {
            outbounds.push(outbound);
        }
    }

    if outbounds.is_empty() {
        bail!("没有识别到 V2Ray URI 节点");
    }

    Ok(json_outbounds(outbounds))
}

fn parse_proxy_uri(line: &str) -> Option<Value> {
    if line.starts_with("vmess://") {
        parse_vmess_uri(line)
    } else if line.starts_with("vless://") {
        parse_standard_uri(line, "vless")
    } else if line.starts_with("trojan://") {
        parse_standard_uri(line, "trojan")
    } else if line.starts_with("ss://") {
        parse_ss_uri(line)
    } else if line.starts_with("hysteria2://") || line.starts_with("hy2://") {
        parse_hysteria2_uri(line)
    } else if line.starts_with("tuic://") {
        parse_tuic_uri(line)
    } else {
        None
    }
}

fn parse_vmess_uri(line: &str) -> Option<Value> {
    let payload = line.strip_prefix("vmess://")?;
    let decoded = decode_base64_text(payload)?;
    let value = serde_json::from_str::<Value>(&decoded).ok()?;
    let tag = value.get("ps").and_then(Value::as_str).unwrap_or("VMess");
    let server = value.get("add").and_then(Value::as_str)?;
    let port = value.get("port").and_then(|v| {
        v.as_str()
            .and_then(|s| s.parse::<u16>().ok())
            .or_else(|| v.as_u64().map(|n| n as u16))
    })?;
    let uuid = value.get("id").and_then(Value::as_str)?;
    let mut outbound = json!({
        "type": "vmess",
        "tag": tag,
        "server": server,
        "server_port": port,
        "uuid": uuid,
        "security": value.get("scy").and_then(Value::as_str).unwrap_or("auto"),
        "alter_id": value.get("aid").and_then(|v| v.as_str().and_then(|s| s.parse::<i64>().ok()).or_else(|| v.as_i64())).unwrap_or(0)
    });

    let tls = value.get("tls").and_then(Value::as_str).unwrap_or_default();
    if tls == "tls" {
        outbound["tls"] = json!({
            "enabled": true,
            "server_name": value.get("sni").and_then(Value::as_str).or_else(|| value.get("host").and_then(Value::as_str)).unwrap_or(server)
        });
    }
    match value.get("net").and_then(Value::as_str).unwrap_or_default() {
        "ws" => {
            let mut transport = json!({
                "type": "ws",
                "path": value.get("path").and_then(Value::as_str).unwrap_or("/")
            });
            if let Some(host) = value
                .get("host")
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
            {
                transport["headers"] = json!({ "Host": host });
            }
            outbound["transport"] = transport;
        }
        "grpc" => {
            outbound["transport"] = json!({
                "type": "grpc",
                "service_name": value.get("path").and_then(Value::as_str).unwrap_or_default()
            });
        }
        _ => {}
    }
    Some(outbound)
}

fn parse_standard_uri(line: &str, outbound_type: &str) -> Option<Value> {
    let url = Url::parse(line).ok()?;
    let tag = url
        .fragment()
        .and_then(|v| urlencoding::decode(v).ok())
        .map(|v| v.into_owned())
        .unwrap_or_else(|| outbound_type.to_owned());
    let server = url.host_str()?.to_owned();
    let port = url.port()?;
    let user = url.username();
    let password = url.password();
    let query = query_map(&url);

    let mut outbound = match outbound_type {
        "vless" => json!({
            "type": "vless",
            "tag": tag,
            "server": server,
            "server_port": port,
            "uuid": user
        }),
        "trojan" => json!({
            "type": "trojan",
            "tag": tag,
            "server": server,
            "server_port": port,
            "password": user
        }),
        _ => return None,
    };

    if outbound_type == "vless" {
        if let Some(flow) = query.get("flow") {
            outbound["flow"] = normalize_flow_value(flow).into();
        }
    }
    apply_uri_tls(&mut outbound, &query, &server);
    apply_uri_transport(&mut outbound, &query);
    if let Some(password) = password.filter(|v| !v.is_empty()) {
        outbound["password"] = password.into();
    }
    Some(outbound)
}

fn parse_ss_uri(line: &str) -> Option<Value> {
    let without_scheme = line.strip_prefix("ss://")?;
    let (main, fragment) = split_once(without_scheme, '#')
        .map(|(main, fragment)| (main, Some(fragment)))
        .unwrap_or((without_scheme, None));
    let tag = fragment
        .and_then(|value| urlencoding::decode(value).ok())
        .map(|value| value.into_owned())
        .unwrap_or_else(|| "Shadowsocks".to_owned());
    let (main, query) = split_once(main, '?')
        .map(|(main, query)| (main, Some(query)))
        .unwrap_or((main, None));
    let decoded_main = if main.contains('@') {
        main.to_owned()
    } else {
        decode_base64_text(main)?
    };
    let (userinfo, hostport) = split_once(&decoded_main, '@')?;
    let userinfo = decode_base64_text(userinfo).unwrap_or_else(|| userinfo.to_owned());
    let (method, password) = split_once(&userinfo, ':')?;
    let (server, port) = split_host_port(hostport)?;

    let mut outbound = json!({
        "type": "shadowsocks",
        "tag": tag,
        "server": server,
        "server_port": port,
        "method": method,
        "password": password
    });

    if let Some(query) = query {
        let fake_url = Url::parse(&format!("ss://x?{query}")).ok()?;
        let query = query_map(&fake_url);
        if let Some(plugin) = query.get("plugin") {
            if plugin.starts_with("v2ray-plugin") {
                outbound["plugin"] = "v2ray-plugin".into();
                outbound["plugin_opts"] = plugin.clone().into();
            }
        }
    }

    Some(outbound)
}

fn parse_hysteria2_uri(line: &str) -> Option<Value> {
    let url = Url::parse(line).ok()?;
    let query = query_map(&url);
    let server = url.host_str()?.to_owned();
    let tag = uri_fragment_tag(&url, "Hysteria2");
    let mut outbound = json!({
        "type": "hysteria2",
        "tag": tag,
        "server": server,
        "server_port": url.port().unwrap_or(443),
        "password": percent_decode(url.username())
    });

    if let Some(obfs_type) = query_get_any(&query, &["obfs", "obfs-type", "obfs_type"]) {
        outbound["obfs"] = json!({
            "type": if obfs_type.is_empty() { "salamander" } else { obfs_type },
            "password": query_get_any(&query, &["obfs-password", "obfs_password", "obfsPassword"])
                .unwrap_or_default()
        });
    } else if let Some(obfs_password) =
        query_get_any(&query, &["obfs-password", "obfs_password", "obfsPassword"])
    {
        outbound["obfs"] = json!({
            "type": "salamander",
            "password": obfs_password
        });
    }

    apply_hysteria_tuic_tls(&mut outbound, &query, &server);
    Some(outbound)
}

fn parse_tuic_uri(line: &str) -> Option<Value> {
    let url = Url::parse(line).ok()?;
    let query = query_map(&url);
    let server = url.host_str()?.to_owned();
    let tag = uri_fragment_tag(&url, "TUIC");
    let mut outbound = json!({
        "type": "tuic",
        "tag": tag,
        "server": server,
        "server_port": url.port().unwrap_or(443),
        "uuid": percent_decode(url.username()),
        "password": percent_decode(url.password().unwrap_or_default())
    });

    if let Some(value) = query_get_any(
        &query,
        &[
            "congestion_control",
            "congestion-controller",
            "congestion-control",
            "congestionControl",
        ],
    ) {
        outbound["congestion_control"] = value.into();
    }
    if let Some(value) = query_get_any(
        &query,
        &["udp_relay_mode", "udp-relay-mode", "udpRelayMode"],
    ) {
        outbound["udp_relay_mode"] = value.into();
    }

    apply_hysteria_tuic_tls(&mut outbound, &query, &server);
    Some(outbound)
}

fn apply_uri_tls(outbound: &mut Value, query: &HashMap<String, String>, server: &str) {
    let security = query
        .get("security")
        .map(String::as_str)
        .unwrap_or_default();
    if !matches!(security, "tls" | "reality") {
        return;
    }
    let mut tls = json!({
        "enabled": true,
        "server_name": query.get("sni").or_else(|| query.get("serverName")).map(String::as_str).unwrap_or(server)
    });
    if let Some(fp) = query.get("fp") {
        tls["utls"] = json!({
            "enabled": true,
            "fingerprint": fp
        });
    }
    if security == "reality" {
        tls["reality"] = json!({
            "enabled": true,
            "public_key": query.get("pbk").map(String::as_str).unwrap_or_default(),
            "short_id": query.get("sid").map(String::as_str).unwrap_or_default()
        });
    }
    ensure_reality_utls_value(
        &mut tls,
        query.get("fp").map(String::as_str),
        security == "reality",
    );
    outbound["tls"] = tls;
}

fn apply_hysteria_tuic_tls(outbound: &mut Value, query: &HashMap<String, String>, server: &str) {
    let mut tls = json!({
        "enabled": true,
        "server_name": query_get_any(query, &["sni", "peer", "serverName", "servername"])
            .unwrap_or(server)
    });
    if let Some(insecure) = query_get_any(
        query,
        &[
            "insecure",
            "allowInsecure",
            "allow-insecure",
            "skip-cert-verify",
            "skip_cert_verify",
        ],
    )
    .and_then(parse_boolish)
    {
        tls["insecure"] = insecure.into();
    }
    if let Some(alpn) = query_get_any(query, &["alpn"]) {
        let values: Vec<&str> = alpn
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect();
        if !values.is_empty() {
            tls["alpn"] = values.into();
        }
    }
    outbound["tls"] = tls;
}

fn apply_uri_transport(outbound: &mut Value, query: &HashMap<String, String>) {
    match query.get("type").map(String::as_str).unwrap_or_default() {
        "ws" => {
            let mut transport = json!({
                "type": "ws",
                "path": query.get("path").map(String::as_str).unwrap_or("/")
            });
            if let Some(host) = query.get("host") {
                transport["headers"] = json!({ "Host": host });
            }
            outbound["transport"] = transport;
        }
        "grpc" => {
            outbound["transport"] = json!({
                "type": "grpc",
                "service_name": query.get("serviceName").or_else(|| query.get("service_name")).map(String::as_str).unwrap_or_default()
            });
        }
        _ => {}
    }
}

fn json_outbounds(outbounds: Vec<Value>) -> Value {
    json!({ "outbounds": outbounds })
}

fn yaml_str<'a>(value: &'a serde_yaml::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(serde_yaml::Value::as_str)
}

fn yaml_mapping_str<'a>(value: &'a serde_yaml::Mapping, key: &str) -> Option<&'a str> {
    value
        .get(serde_yaml::Value::String(key.to_owned()))
        .and_then(serde_yaml::Value::as_str)
}

fn yaml_bool(value: &serde_yaml::Value, key: &str) -> Option<bool> {
    value.get(key).and_then(serde_yaml::Value::as_bool)
}

fn yaml_u16(value: &serde_yaml::Value, key: &str) -> Option<u16> {
    value.get(key).and_then(|v| {
        v.as_i64()
            .map(|n| n as u16)
            .or_else(|| v.as_str().and_then(|s| s.parse::<u16>().ok()))
    })
}

fn yaml_i64(value: &serde_yaml::Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|v| {
        v.as_i64()
            .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
    })
}

fn json_obj(items: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(
        items
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn decode_base64_text(value: &str) -> Option<String> {
    let cleaned = value.trim().replace(['\r', '\n', ' '], "");
    let padded = match cleaned.len() % 4 {
        0 => cleaned,
        n => format!("{}{}", cleaned, "=".repeat(4 - n)),
    };
    general_purpose::STANDARD
        .decode(padded.as_bytes())
        .or_else(|_| general_purpose::URL_SAFE.decode(padded.as_bytes()))
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

fn normalize_proxy_line(raw: &str) -> String {
    let mut line = raw.trim().trim_matches('`').trim().to_owned();

    if let Some(stripped) = strip_numbered_prefix(&line) {
        line = stripped.to_owned();
    }

    line = line
        .trim_start_matches(|ch: char| matches!(ch, '-' | '*' | '+' | '>'))
        .trim()
        .trim_matches('`')
        .trim()
        .to_owned();

    line
}

fn strip_numbered_prefix(line: &str) -> Option<&str> {
    let digit_count = line.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digit_count == 0 {
        return None;
    }
    let rest = &line[digit_count..];
    let rest = rest.strip_prefix('.').or_else(|| rest.strip_prefix(')'))?;
    Some(rest.trim_start())
}

fn query_map(url: &Url) -> HashMap<String, String> {
    url.query_pairs().into_owned().collect()
}

fn query_get_any<'a>(query: &'a HashMap<String, String>, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| query.get(*key).map(String::as_str))
        .filter(|value| !value.is_empty())
}

fn uri_fragment_tag(url: &Url, fallback: &str) -> String {
    url.fragment()
        .and_then(|value| urlencoding::decode(value).ok())
        .map(|value| value.into_owned())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback.to_owned())
}

fn percent_decode(value: &str) -> String {
    urlencoding::decode(value)
        .map(|value| value.into_owned())
        .unwrap_or_else(|_| value.to_owned())
}

fn parse_boolish(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

fn split_once(value: &str, needle: char) -> Option<(&str, &str)> {
    value.split_once(needle)
}

fn split_host_port(value: &str) -> Option<(&str, u16)> {
    if let Some(rest) = value.strip_prefix('[') {
        let (host, rest) = rest.split_once(']')?;
        let port = rest.strip_prefix(':')?.parse().ok()?;
        return Some((host, port));
    }
    let (host, port) = value.rsplit_once(':')?;
    if host.contains(':') {
        return None;
    }
    Some((host, port.parse().ok()?))
}

fn remove_if_exists(path: std::path::PathBuf) -> Result<()> {
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

fn ensure_reality_utls(
    tls: &mut serde_json::Map<String, Value>,
    fingerprint: Option<&str>,
    required: bool,
) {
    if !required {
        return;
    }
    let fingerprint = fingerprint
        .filter(|value| !value.is_empty())
        .unwrap_or("chrome");
    tls.insert(
        "utls".into(),
        json!({
            "enabled": true,
            "fingerprint": fingerprint
        }),
    );
}

fn ensure_reality_utls_value(tls: &mut Value, fingerprint: Option<&str>, required: bool) {
    if !required {
        return;
    }
    let fingerprint = fingerprint
        .filter(|value| !value.is_empty())
        .unwrap_or("chrome");
    tls["utls"] = json!({
        "enabled": true,
        "fingerprint": fingerprint
    });
}

fn normalize_outbound_strings(outbound: &mut Value) {
    let Some(object) = outbound.as_object_mut() else {
        return;
    };
    for key in ["tag", "type", "server"] {
        if let Some(value) = object
            .get(key)
            .and_then(Value::as_str)
            .map(|value| value.trim().to_owned())
        {
            object.insert(key.to_owned(), value.into());
        }
    }
}

fn unique_tag(tag: &str, seen_tags: &mut HashMap<String, usize>) -> String {
    let count = seen_tags.entry(tag.to_owned()).or_insert(0);
    *count += 1;
    if *count == 1 {
        tag.to_owned()
    } else {
        format!("{tag} #{}", *count)
    }
}

fn is_importable_outbound(outbound: &Value, tag: &str, outbound_type: &str) -> bool {
    if tag.is_empty() || is_reserved_outbound(tag, outbound_type) || is_informational_tag(tag) {
        return false;
    }

    let server = outbound
        .get("server")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if server.is_empty() || matches!(server, "0.0.0.0" | "::" | "[::]") {
        return false;
    }
    if outbound_u16(outbound, "server_port").is_none() {
        return false;
    }

    match outbound_type {
        "shadowsocks" => {
            has_non_empty_str(outbound, "method") && has_non_empty_str(outbound, "password")
        }
        "vmess" | "vless" => has_non_empty_str(outbound, "uuid"),
        "trojan" | "hysteria2" => has_non_empty_str(outbound, "password"),
        "tuic" => has_non_empty_str(outbound, "uuid") && has_non_empty_str(outbound, "password"),
        _ => true,
    }
}

fn has_non_empty_str(value: &Value, key: &str) -> bool {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn outbound_u16(value: &Value, key: &str) -> Option<u16> {
    value.get(key).and_then(|item| {
        item.as_u64()
            .and_then(|port| u16::try_from(port).ok())
            .or_else(|| item.as_str().and_then(|port| port.parse::<u16>().ok()))
    })
}

fn is_reserved_outbound(tag: &str, outbound_type: &str) -> bool {
    matches!(tag, "DIRECT" | "PROXY" | "GLOBAL" | "REJECT" | "dns-out")
        || matches!(outbound_type, "direct" | "block" | "dns")
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

fn subscription_source_key(source: &str, is_remote_url: bool) -> String {
    if is_remote_url {
        source.to_owned()
    } else {
        format!("manual://import/{}", uuid::Uuid::new_v4())
    }
}

fn subscription_is_remote_url(source: &str) -> bool {
    source.starts_with("http://") || source.starts_with("https://")
}

fn normalize_flow_value(flow: &str) -> String {
    match flow.trim() {
        "xtls-rprx-vision-udp443" => "xtls-rprx-vision".to_owned(),
        other => other.to_owned(),
    }
}

fn infer_name(source: &str, index: usize, is_remote_url: bool) -> String {
    if is_remote_url {
        let host = source
            .split("://")
            .nth(1)
            .and_then(|rest| rest.split('/').next())
            .filter(|host| !host.is_empty())
            .unwrap_or("订阅");
        format!("{host} #{index}")
    } else {
        format!("手动导入 #{index}")
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_markdown_bullet_proxy_lines() {
        let text = "- trojan://Puj01Rc8UcA9IzcFcYOs8KMOhCz6aX2Q@mfyousheng.nl.eu.org:443?security=tls&type=ws&path=%2FtjwsLhx0SFASG4l9FERJ1g#TG-%40vvkj11";
        let value = parse_v2ray_subscription(text).expect("should parse markdown bullet proxy");
        let outbounds = value["outbounds"].as_array().expect("outbounds array");
        assert_eq!(outbounds.len(), 1);
        assert_eq!(outbounds[0]["type"], "trojan");
        assert_eq!(outbounds[0]["transport"]["type"], "ws");
    }

    #[test]
    fn reality_nodes_force_utls() {
        let mut tls = json!({
            "enabled": true,
            "server_name": "www.microsoft.com",
            "reality": {
                "enabled": true,
                "public_key": "test",
                "short_id": ""
            }
        });
        ensure_reality_utls_value(&mut tls, None, true);
        assert_eq!(tls["utls"]["enabled"], true);
        assert_eq!(tls["utls"]["fingerprint"], "chrome");
    }

    #[test]
    fn normalizes_unsupported_flow_suffix() {
        assert_eq!(
            normalize_flow_value("xtls-rprx-vision-udp443"),
            "xtls-rprx-vision"
        );
        assert_eq!(normalize_flow_value("xtls-rprx-vision"), "xtls-rprx-vision");
    }

    #[test]
    fn parses_clash_hysteria2_and_tuic_nodes() {
        let text = r#"
proxies:
  - name: HY2-01
    type: hysteria2
    server: hy.example.com
    port: 443
    password: hy-pass
    sni: edge.example.com
    skip-cert-verify: true
    obfs: salamander
    obfs-password: obfs-pass
  - name: TUIC-01
    type: tuic
    server: tuic.example.com
    port: 443
    uuid: 00000000-0000-0000-0000-000000000000
    password: tuic-pass
    sni: tuic-sni.example.com
    congestion-controller: bbr
    udp-relay-mode: native
"#;

        let value = parse_clash_yaml(text).expect("should parse clash yaml");
        let outbounds = value["outbounds"].as_array().expect("outbounds");
        assert_eq!(outbounds.len(), 2);
        assert_eq!(outbounds[0]["type"], "hysteria2");
        assert_eq!(outbounds[0]["tls"]["server_name"], "edge.example.com");
        assert_eq!(outbounds[0]["tls"]["insecure"], true);
        assert_eq!(outbounds[0]["obfs"]["password"], "obfs-pass");
        assert_eq!(outbounds[1]["type"], "tuic");
        assert_eq!(outbounds[1]["congestion_control"], "bbr");
        assert_eq!(outbounds[1]["udp_relay_mode"], "native");
    }

    #[test]
    fn parses_hysteria2_uri() {
        let outbound = parse_proxy_uri(
            "hy2://hy-pass@hy.example.com:443?sni=edge.example.com&insecure=1&obfs=salamander&obfs-password=obfs-pass#HY2-URI",
        )
        .expect("should parse hysteria2 uri");

        assert_eq!(outbound["type"], "hysteria2");
        assert_eq!(outbound["tag"], "HY2-URI");
        assert_eq!(outbound["server"], "hy.example.com");
        assert_eq!(outbound["password"], "hy-pass");
        assert_eq!(outbound["tls"]["server_name"], "edge.example.com");
        assert_eq!(outbound["tls"]["insecure"], true);
        assert_eq!(outbound["obfs"]["type"], "salamander");
        assert_eq!(outbound["obfs"]["password"], "obfs-pass");
    }

    #[test]
    fn parses_tuic_uri() {
        let outbound = parse_proxy_uri(
            "tuic://00000000-0000-0000-0000-000000000000:tuic-pass@tuic.example.com:443?sni=tuic-sni.example.com&congestion_control=bbr&udp_relay_mode=native#TUIC-URI",
        )
        .expect("should parse tuic uri");

        assert_eq!(outbound["type"], "tuic");
        assert_eq!(outbound["tag"], "TUIC-URI");
        assert_eq!(outbound["uuid"], "00000000-0000-0000-0000-000000000000");
        assert_eq!(outbound["password"], "tuic-pass");
        assert_eq!(outbound["tls"]["server_name"], "tuic-sni.example.com");
        assert_eq!(outbound["congestion_control"], "bbr");
        assert_eq!(outbound["udp_relay_mode"], "native");
    }

    #[test]
    fn sanitize_converted_subscription_removes_information_outbounds() {
        let mut value = json!({
            "outbounds": [
                { "type": "vless", "tag": "\u{5269}\u{4f59}\u{6d41}\u{91cf}: 10GB" },
                { "type": "vless", "tag": "https://example.com" },
                { "type": "direct", "tag": "DIRECT" },
                { "type": "vless", "tag": "HK-01", "server": "example.com", "server_port": 443, "uuid": "00000000-0000-0000-0000-000000000000" }
            ]
        });

        sanitize_converted_subscription(&mut value);
        let outbounds = value["outbounds"].as_array().expect("outbounds");
        assert_eq!(outbounds.len(), 1);
        assert_eq!(outbounds[0]["tag"], "HK-01");
    }

    #[test]
    fn sanitize_converted_subscription_renames_duplicates_and_drops_bad_nodes() {
        let mut value = json!({
            "outbounds": [
                { "type": "vless", "tag": " HK-01 ", "server": " example.com ", "server_port": 443, "uuid": "00000000-0000-0000-0000-000000000000" },
                { "type": "vless", "tag": "HK-01", "server": "example.org", "server_port": "443", "uuid": "00000000-0000-0000-0000-000000000001" },
                { "type": "vless", "tag": "placeholder", "server": "0.0.0.0", "server_port": 443, "uuid": "00000000-0000-0000-0000-000000000002" },
                { "type": "vless", "tag": "missing-port", "server": "example.net", "uuid": "00000000-0000-0000-0000-000000000003" },
                { "type": "selector", "tag": "not-a-node", "outbounds": ["HK-01"] }
            ]
        });

        sanitize_converted_subscription(&mut value);
        let outbounds = value["outbounds"].as_array().expect("outbounds");
        assert_eq!(outbounds.len(), 2);
        assert_eq!(outbounds[0]["tag"], "HK-01");
        assert_eq!(outbounds[0]["server"], "example.com");
        assert_eq!(outbounds[1]["tag"], "HK-01 #2");
    }
}
