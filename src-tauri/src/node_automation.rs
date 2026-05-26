use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use futures_util::{stream, StreamExt};
use serde_json::Value;
use tauri::AppHandle;
use url::Url;

use crate::{
    clash_api, config,
    models::{
        AppSettings, AutoSelectedProxy, SpeedTestInterval, SpeedTestResult, SpeedTestSummary,
    },
};

const FAIL_THRESHOLD_MS: u64 = 1500;
const DEFAULT_DELAY_TEST_URL: &str = "https://connectivitycheck.gstatic.com/generate_204";
const MAX_DELAY_TEST_TIMEOUT_MS: u64 = 30000;
const MAX_DELAY_TEST_CONCURRENCY: usize = 16;
const MAX_DELAY_TEST_SAMPLES: u8 = 5;

#[derive(Debug, Clone)]
struct DelayTestOptions {
    test_url: String,
    timeout_ms: u64,
    concurrency: usize,
    samples: u8,
}

pub async fn run_speed_test(
    app: &AppHandle,
    select_fastest: bool,
    auto_switch_on_failure: bool,
) -> Result<SpeedTestSummary> {
    let settings = config::load_or_create_settings(app)?;
    let client = clash_api::Client::from_settings(&settings);
    let proxy_list = client.list_proxies().await?;
    let proxies = proxy_list
        .raw
        .get("proxies")
        .and_then(Value::as_object)
        .context("proxy list missing proxies object")?;

    let groups = collect_proxy_groups(proxies);
    let mut node_names = BTreeSet::new();
    for group in &groups {
        for node_name in &group.nodes {
            node_names.insert(node_name.clone());
        }
    }

    let results = measure_node_delays(
        client.clone(),
        node_names.into_iter().collect(),
        speed_test_options(&settings),
        now_unix(),
    )
    .await;
    let mut delay_map = BTreeMap::new();
    for result in &results {
        if let Some(delay) = result.delay {
            delay_map.insert(result.name.clone(), delay);
        }
    }

    save_speed_test_cache(app, &results)?;

    let mut selected = Vec::new();
    if select_fastest {
        for group in groups {
            let Some((best_name, best_delay)) = group
                .nodes
                .iter()
                .filter_map(|name| delay_map.get(name).map(|delay| (name, *delay)))
                .min_by_key(|(_, delay)| *delay)
            else {
                continue;
            };

            if group.now.as_deref() == Some(best_name.as_str()) {
                continue;
            }

            if client.select_proxy(&group.name, best_name).await.is_ok() {
                selected.push(AutoSelectedProxy {
                    group: group.name,
                    name: best_name.clone(),
                    delay: best_delay,
                });
            }
        }
    } else if auto_switch_on_failure {
        for group in groups.into_iter().filter(|group| group.name == "PROXY") {
            let current_delay = group
                .now
                .as_deref()
                .and_then(|name| delay_map.get(name))
                .copied();
            if !is_failure_delay(current_delay) {
                continue;
            }

            let Some((best_name, best_delay)) = group
                .nodes
                .iter()
                .filter_map(|name| delay_map.get(name).map(|delay| (name, *delay)))
                .min_by_key(|(_, delay)| *delay)
            else {
                continue;
            };

            if group.now.as_deref() == Some(best_name.as_str()) {
                continue;
            }

            if client.select_proxy(&group.name, best_name).await.is_ok() {
                selected.push(AutoSelectedProxy {
                    group: group.name,
                    name: best_name.clone(),
                    delay: best_delay,
                });
            }
        }
    }

    let succeeded = results
        .iter()
        .filter(|result| result.delay.is_some())
        .count();
    let failed = results.len().saturating_sub(succeeded);

    Ok(SpeedTestSummary {
        tested: results.len(),
        succeeded,
        failed,
        selected,
        results,
    })
}

pub async fn run_speed_test_for_nodes(
    app: &AppHandle,
    node_names: Vec<String>,
) -> Result<Vec<SpeedTestResult>> {
    let settings = config::load_or_create_settings(app)?;
    let client = clash_api::Client::from_settings(&settings);
    let results = measure_node_delays(
        client,
        node_names,
        speed_test_options(&settings),
        now_unix(),
    )
    .await;
    merge_speed_test_cache(app, &results)?;
    Ok(results)
}

pub fn load_speed_test_cache(app: &AppHandle) -> Result<Vec<SpeedTestResult>> {
    let path = config::speed_test_cache_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read speed test cache {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse speed test cache {}", path.display()))
}

pub fn latest_speed_test_at(app: &AppHandle) -> Result<Option<u64>> {
    Ok(load_speed_test_cache(app)?
        .into_iter()
        .map(|result| result.tested_at)
        .max())
}

pub fn speed_test_interval_secs(interval: SpeedTestInterval) -> Option<u64> {
    match interval {
        SpeedTestInterval::Never => None,
        SpeedTestInterval::Every30Minutes => Some(30 * 60),
        SpeedTestInterval::Every1Hour => Some(60 * 60),
        SpeedTestInterval::Every24Hours => Some(24 * 60 * 60),
    }
}

fn save_speed_test_cache(app: &AppHandle, results: &[SpeedTestResult]) -> Result<()> {
    let path = config::speed_test_cache_path(app)?;
    let content =
        serde_json::to_string_pretty(results).context("failed to serialize speed test cache")?;
    fs::write(&path, content)
        .with_context(|| format!("failed to write speed test cache {}", path.display()))
}

fn merge_speed_test_cache(app: &AppHandle, results: &[SpeedTestResult]) -> Result<()> {
    let mut latest = load_speed_test_cache(app)?
        .into_iter()
        .map(|result| (result.name.clone(), result))
        .collect::<BTreeMap<_, _>>();
    for result in results {
        latest.insert(result.name.clone(), result.clone());
    }
    let merged = latest.into_values().collect::<Vec<_>>();
    save_speed_test_cache(app, &merged)
}

async fn measure_node_delays(
    client: clash_api::Client,
    node_names: Vec<String>,
    options: DelayTestOptions,
    tested_at: u64,
) -> Vec<SpeedTestResult> {
    let node_names = dedupe_node_names(node_names);
    stream::iter(node_names.into_iter().map(|name| {
        let client = client.clone();
        let test_url = options.test_url.clone();
        async move {
            measure_proxy_delay(
                client,
                name,
                test_url,
                options.timeout_ms,
                options.samples,
                tested_at,
            )
            .await
        }
    }))
    .buffer_unordered(options.concurrency)
    .collect()
    .await
}

async fn measure_proxy_delay(
    client: clash_api::Client,
    name: String,
    test_url: String,
    timeout_ms: u64,
    samples: u8,
    tested_at: u64,
) -> SpeedTestResult {
    let mut delays = Vec::new();
    let mut last_error = None;
    for _ in 0..samples.max(1) {
        match client
            .delay_proxy_with_options(&name, &test_url, timeout_ms)
            .await
        {
            Ok(delay) => delays.push(delay),
            Err(error) => last_error = Some(error.to_string()),
        }
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    }

    let delay = median_u64(delays);
    let error = if delay.is_some() {
        None
    } else {
        last_error.or_else(|| Some("no delay result".to_owned()))
    };

    SpeedTestResult {
        name,
        delay,
        tested_at,
        error,
    }
}

fn dedupe_node_names(node_names: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    node_names
        .into_iter()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty() && seen.insert(name.clone()))
        .collect()
}

fn median_u64(mut values: Vec<u64>) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    Some(values[values.len() / 2])
}

fn speed_test_options(settings: &AppSettings) -> DelayTestOptions {
    DelayTestOptions {
        test_url: normalize_test_url(&settings.speed_test_url),
        timeout_ms: settings
            .speed_test_timeout_ms
            .clamp(1000, MAX_DELAY_TEST_TIMEOUT_MS),
        concurrency: settings
            .speed_test_concurrency
            .clamp(1, MAX_DELAY_TEST_CONCURRENCY),
        samples: settings.speed_test_samples.clamp(1, MAX_DELAY_TEST_SAMPLES),
    }
}

fn normalize_test_url(candidate: &str) -> String {
    if let Ok(parsed) = Url::parse(candidate.trim()) {
        if matches!(parsed.scheme(), "http" | "https") {
            return candidate.trim().to_owned();
        }
    }
    DEFAULT_DELAY_TEST_URL.to_owned()
}

fn collect_proxy_groups(proxies: &serde_json::Map<String, Value>) -> Vec<ProxyGroup> {
    proxies
        .values()
        .filter_map(|proxy| {
            let name = proxy.get("name")?.as_str()?.to_owned();
            if name == "GLOBAL" {
                return None;
            }
            let all = proxy.get("all")?.as_array()?;
            let nodes = all
                .iter()
                .filter_map(Value::as_str)
                .filter(|name| is_testable_proxy(proxies, name))
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if nodes.is_empty() {
                return None;
            }

            Some(ProxyGroup {
                name,
                now: proxy.get("now").and_then(Value::as_str).map(str::to_owned),
                nodes,
            })
        })
        .collect()
}

fn is_testable_proxy(proxies: &serde_json::Map<String, Value>, name: &str) -> bool {
    if matches!(name, "DIRECT" | "REJECT") || is_informational_node_name(name) {
        return false;
    }
    let Some(proxy) = proxies.get(name) else {
        return false;
    };
    if proxy.get("all").and_then(Value::as_array).is_some() {
        return false;
    }
    let proxy_type = proxy
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    !matches!(proxy_type, "Direct" | "Reject")
}

fn is_informational_node_name(name: &str) -> bool {
    let normalized = name.trim().to_lowercase();
    if normalized.is_empty() {
        return true;
    }

    let prefixes = [
        "剩余流量",
        "距离下次重置剩余",
        "套餐到期",
        "官网",
        "备用网址",
        "跳转域名",
        "请勿连接",
        "客服",
        "更新地址",
        "到期时间",
        "过期时间",
        "流量",
        "有效期",
        "联系",
        "电报群",
        "telegram",
        "tg群",
        "群组",
        "公告",
        "提示",
        "说明",
        "订阅",
        "获取更多",
        "购买",
        "续费",
        "网址",
        "http://",
        "https://",
    ];

    prefixes.iter().any(|prefix| normalized.starts_with(prefix)) || normalized.contains("请勿连接")
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn is_failure_delay(delay: Option<u64>) -> bool {
    match delay {
        None => true,
        Some(value) => value >= FAIL_THRESHOLD_MS,
    }
}

struct ProxyGroup {
    name: String,
    now: Option<String>,
    nodes: Vec<String>,
}
