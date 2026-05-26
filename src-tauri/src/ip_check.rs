use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context};
use reqwest::header::{ACCEPT, USER_AGENT};
use serde_json::Value;

use crate::models::{
    IpCheckCard, IpCheckRequest, IpCheckResult, IpConnectivityResult, IpConnectivityTarget,
};

const IPCHECK_IPV4_URL: &str = "https://4.ipcheck.ing/";
const IPCHECK_API_URL: &str = "https://ipcheck.ing/api/ipchecking";
const IPSB_GEOIP_URL: &str = "https://api.ip.sb/geoip";
const BROWSERLEAKS_IP_URL: &str = "https://browserleaks.com/ip";

const CONNECTIVITY_TARGETS: &[(&str, &str, &str)] = &[
    (
        "wechat",
        "WeChat",
        "https://res.wx.qq.com/a/wx_fed/assets/res/NTI4MWU5.ico",
    ),
    ("taobao", "Taobao", "https://www.taobao.com/favicon.ico"),
    ("google", "Google", "https://www.google.com/generate_204"),
    (
        "cloudflare",
        "Cloudflare",
        "https://www.cloudflare.com/favicon.ico",
    ),
    ("youtube", "YouTube", "https://www.youtube.com/generate_204"),
    (
        "github",
        "GitHub",
        "https://github.githubassets.com/favicons/favicon.svg",
    ),
    ("chatgpt", "ChatGPT", "https://chatgpt.com/favicon.ico"),
];

pub async fn run_ip_check(request: IpCheckRequest) -> anyhow::Result<IpCheckResult> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) WEPBOX/0.1")
        .build()
        .context("failed to build IP check client")?;

    let ipv4 = fetch_ipv4_card(&client).await.unwrap_or_else(error_card_v4);
    let connectivity = run_connectivity_tests(&client, request.custom_targets).await;

    Ok(IpCheckResult {
        checked_at: now_unix(),
        ipv4,
        connectivity,
    })
}

async fn fetch_ipv4_card(client: &reqwest::Client) -> anyhow::Result<IpCheckCard> {
    match fetch_ipcheck_card(client, "IPv4", IPCHECK_IPV4_URL).await {
        Ok(card) => Ok(card),
        Err(_) => match fetch_ip_sb_card(client).await {
            Ok(card) => Ok(card),
            Err(_) => fetch_browserleaks_card(client).await,
        },
    }
}

async fn fetch_ipcheck_card(
    client: &reqwest::Client,
    version: &str,
    ip_url: &str,
) -> anyhow::Result<IpCheckCard> {
    let ip_value: Value = client
        .get(ip_url)
        .header(ACCEPT, "application/json,text/plain,*/*")
        .send()
        .await
        .with_context(|| format!("failed to request IPCheck.ing {version} endpoint"))?
        .error_for_status()
        .with_context(|| format!("IPCheck.ing {version} endpoint returned an error"))?
        .json()
        .await
        .with_context(|| format!("failed to parse IPCheck.ing {version} response"))?;
    let ip =
        json_text(&ip_value, "ip").ok_or_else(|| anyhow!("IPCheck.ing did not return an IP"))?;

    let value: Value = client
        .get(IPCHECK_API_URL)
        .query(&[("ip", ip.as_str()), ("lang", "zh-CN")])
        .header(ACCEPT, "application/json,text/plain,*/*")
        .send()
        .await
        .context("failed to request IPCheck.ing detail endpoint")?
        .error_for_status()
        .context("IPCheck.ing detail endpoint returned an error")?
        .json()
        .await
        .context("failed to parse IPCheck.ing detail response")?;

    Ok(card_from_ipcheck(version, Some(ip), &value))
}

async fn fetch_ip_sb_card(client: &reqwest::Client) -> anyhow::Result<IpCheckCard> {
    let value: Value = client
        .get(IPSB_GEOIP_URL)
        .header(ACCEPT, "application/json")
        .send()
        .await
        .context("failed to request ip.sb")?
        .error_for_status()
        .context("ip.sb returned an error")?
        .json()
        .await
        .context("failed to parse ip.sb response")?;

    Ok(IpCheckCard {
        version: "IPv4".to_owned(),
        source: "ip.sb".to_owned(),
        available: true,
        ip: json_text(&value, "ip"),
        country: json_text(&value, "country"),
        region: None,
        city: json_text(&value, "city"),
        network: json_text(&value, "isp").or_else(|| json_text(&value, "organization")),
        usage_type: None,
        proxy: None,
        native: None,
        quality_score: None,
        asn: json_text(&value, "asn").map(|asn| format_asn(&asn)),
        message: Some("IPCheck.ing 被验证保护时使用的基础地理信息兜底。".to_owned()),
    })
}

async fn fetch_browserleaks_card(client: &reqwest::Client) -> anyhow::Result<IpCheckCard> {
    let html = client
        .get(BROWSERLEAKS_IP_URL)
        .header(ACCEPT, "text/html,application/xhtml+xml")
        .send()
        .await
        .context("failed to request BrowserLeaks")?
        .error_for_status()
        .context("BrowserLeaks returned an error")?
        .text()
        .await
        .context("failed to read BrowserLeaks response")?;

    Ok(IpCheckCard {
        version: "IPv4".to_owned(),
        source: "BrowserLeaks".to_owned(),
        available: true,
        ip: extract_browserleaks_value(&html, "IP Address"),
        country: extract_browserleaks_value(&html, "Country"),
        region: extract_browserleaks_value(&html, "State/Region"),
        city: extract_browserleaks_value(&html, "City"),
        network: extract_browserleaks_value(&html, "ISP"),
        usage_type: extract_browserleaks_value(&html, "Usage Type"),
        proxy: None,
        native: None,
        quality_score: None,
        asn: extract_browserleaks_value(&html, "Network").and_then(|value| {
            value
                .split_whitespace()
                .find(|part| part.starts_with("AS"))
                .map(ToOwned::to_owned)
        }),
        message: Some("BrowserLeaks 可提供更多浏览器检测项，但这里仅抽取 IP 卡片字段。".to_owned()),
    })
}

fn card_from_ipcheck(version: &str, detected_ip: Option<String>, value: &Value) -> IpCheckCard {
    let advanced = value.get("advancedData").unwrap_or(&Value::Null);
    IpCheckCard {
        version: version.to_owned(),
        source: format!("IPCheck.ing {version}"),
        available: true,
        ip: detected_ip.or_else(|| json_text(value, "ip")),
        country: json_text(value, "country_name").or_else(|| json_text(value, "country")),
        region: json_text(value, "region"),
        city: json_text(value, "city"),
        network: json_text(value, "org"),
        usage_type: advanced_text(advanced, "operatorType").map(normalize_restricted_value),
        proxy: proxy_text(advanced),
        native: native_text(advanced),
        quality_score: advanced_text(advanced, "score").map(normalize_score),
        asn: json_text(value, "asn").map(|asn| format_asn(&asn)),
        message: None,
    }
}

async fn run_connectivity_tests(
    client: &reqwest::Client,
    custom_targets: Vec<IpConnectivityTarget>,
) -> Vec<IpConnectivityResult> {
    let mut targets: Vec<IpConnectivityTarget> = CONNECTIVITY_TARGETS
        .iter()
        .map(|(id, name, url)| IpConnectivityTarget {
            id: (*id).to_owned(),
            name: (*name).to_owned(),
            url: (*url).to_owned(),
        })
        .collect();
    targets.extend(
        custom_targets
            .into_iter()
            .filter(|target| valid_connectivity_target(target)),
    );

    let mut tasks = tokio::task::JoinSet::new();
    for (index, target) in targets.into_iter().enumerate() {
        let client = client.clone();
        tasks.spawn(async move {
            (
                index,
                test_connectivity(client, &target.id, &target.name, &target.url).await,
            )
        });
    }

    let mut results = Vec::with_capacity(CONNECTIVITY_TARGETS.len());
    while let Some(result) = tasks.join_next().await {
        if let Ok(item) = result {
            results.push(item);
        }
    }
    results.sort_by_key(|(index, _)| *index);
    let results = results.into_iter().map(|(_, item)| item).collect();
    results
}

async fn test_connectivity(
    client: reqwest::Client,
    id: &str,
    name: &str,
    url: &str,
) -> IpConnectivityResult {
    let started = Instant::now();
    let result = client
        .get(url)
        .header(
            USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) WEPBOX/0.1",
        )
        .send()
        .await;
    let elapsed = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

    match result {
        Ok(response) if response.status().is_success() || response.status().is_redirection() => {
            let status = if elapsed <= 800 { "ok" } else { "slow" };
            IpConnectivityResult {
                id: id.to_owned(),
                name: name.to_owned(),
                status: status.to_owned(),
                latency_ms: Some(elapsed),
                message: None,
            }
        }
        Ok(response) => IpConnectivityResult {
            id: id.to_owned(),
            name: name.to_owned(),
            status: "blocked".to_owned(),
            latency_ms: Some(elapsed),
            message: Some(format!("HTTP {}", response.status().as_u16())),
        },
        Err(error) => IpConnectivityResult {
            id: id.to_owned(),
            name: name.to_owned(),
            status: "blocked".to_owned(),
            latency_ms: None,
            message: Some(error.to_string()),
        },
    }
}

fn proxy_text(advanced: &Value) -> Option<String> {
    let tags = advanced.get("tags")?;
    if tags == "sign_in_required" {
        return Some("需要登录后查看".to_owned());
    }
    match tags.get("isProxyOrVPN").and_then(Value::as_bool) {
        Some(true) => Some("可能是代理".to_owned()),
        Some(false) => Some("未识别为代理".to_owned()),
        None => None,
    }
}

fn native_text(advanced: &Value) -> Option<String> {
    let tags = advanced.get("tags")?;
    if tags == "sign_in_required" {
        return Some("需要登录后查看".to_owned());
    }
    match tags.get("isNative").and_then(Value::as_bool) {
        Some(true) => Some("原生".to_owned()),
        Some(false) => Some("非原生".to_owned()),
        None => None,
    }
}

fn normalize_score(value: String) -> String {
    if value == "sign_in_required" {
        "需要登录后查看".to_owned()
    } else if value.contains('/') {
        value
    } else {
        format!("{value}/100")
    }
}

fn normalize_restricted_value(value: String) -> String {
    if value == "sign_in_required" {
        "需要登录后查看".to_owned()
    } else {
        value
    }
}

fn error_card_v4(error: anyhow::Error) -> IpCheckCard {
    error_card("IPv4", error)
}

fn error_card(version: &str, error: anyhow::Error) -> IpCheckCard {
    IpCheckCard {
        version: version.to_owned(),
        source: format!("{version} 检测"),
        available: false,
        ip: None,
        country: None,
        region: None,
        city: None,
        network: None,
        usage_type: None,
        proxy: None,
        native: None,
        quality_score: None,
        asn: None,
        message: Some(error.to_string()),
    }
}

fn valid_connectivity_target(target: &IpConnectivityTarget) -> bool {
    let id = target.id.trim();
    let name = target.name.trim();
    let url = target.url.trim();
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|char| char.is_ascii_alphanumeric() || char == '-' || char == '_')
        && !name.is_empty()
        && name.chars().count() <= 32
        && url.len() <= 512
        && (url.starts_with("https://") || url.starts_with("http://"))
}

fn extract_browserleaks_value(html: &str, label: &str) -> Option<String> {
    let label_index = html.find(&format!("<td>{label}</td>"))?;
    let rest = &html[label_index..];
    let label_end = rest.find("</td>")?;
    let rest = &rest[label_end + 5..];
    let value_start = rest.find("<td")?;
    let rest = &rest[value_start + 3..];
    let value_open_end = rest.find('>')?;
    let rest = &rest[value_open_end + 1..];
    let value_end = rest.find("</td>")?;
    let text = clean_html_text(&rest[..value_end]);
    (!text.is_empty()).then_some(text)
}

fn clean_html_text(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;

    for char in value.chars() {
        match char {
            '<' => {
                in_tag = true;
                output.push(' ');
            }
            '>' => in_tag = false,
            _ if !in_tag => output.push(char),
            _ => {}
        }
    }

    output
        .replace("&nbsp;", " ")
        .replace("&#160;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn json_text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .map(json_value_text)
        .filter(|text| !text.is_empty())
}

fn advanced_text(value: &Value, key: &str) -> Option<String> {
    json_text(value, key)
}

fn json_value_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(item) => item.to_string(),
        Value::Number(item) => item.to_string(),
        Value::String(item) => item.clone(),
        _ => value.to_string(),
    }
}

fn format_asn(value: &str) -> String {
    if value.starts_with("AS") {
        value.to_owned()
    } else {
        format!("AS{value}")
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
