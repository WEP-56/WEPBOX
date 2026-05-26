use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::Deserialize;
use serde_json::{json, Value};
use url::Url;

use crate::models::{AppSettings, ProxyList};

const DELAY_TEST_URL: &str = "https://www.gstatic.com/generate_204";

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
}

impl Client {
    pub fn from_settings(settings: &AppSettings) -> Self {
        let mut headers = HeaderMap::new();
        if !settings.clash_api_secret.is_empty() {
            let value = format!("Bearer {}", settings.clash_api_secret);
            if let Ok(value) = HeaderValue::from_str(&value) {
                headers.insert(AUTHORIZATION, value);
            }
        }

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(3))
            .build()
            .expect("reqwest client should build");

        Self {
            http,
            base_url: settings.api_base_url(),
        }
    }

    pub async fn list_proxies(&self) -> Result<ProxyList> {
        let raw = self
            .http
            .get(self.url("/proxies"))
            .send()
            .await
            .context("failed to request proxy list")?
            .error_for_status()
            .context("proxy list request failed")?
            .json::<Value>()
            .await
            .context("failed to parse proxy list")?;

        Ok(ProxyList { raw })
    }

    pub async fn wait_until_ready(&self) -> Result<()> {
        let mut last_error = None;

        for _ in 0..20 {
            match self.list_proxies().await {
                Ok(_) => return Ok(()),
                Err(error) => {
                    last_error = Some(error.to_string());
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            }
        }

        if let Some(error) = last_error {
            bail!("sing-box API did not become ready: {error}");
        }

        bail!("sing-box API did not become ready");
    }

    pub async fn select_proxy(&self, group: &str, name: &str) -> Result<()> {
        let path = format!("/proxies/{}", urlencoding::encode(group));
        self.http
            .put(self.url(&path))
            .json(&json!({ "name": name }))
            .send()
            .await
            .context("failed to request proxy switch")?
            .error_for_status()
            .context("proxy switch request failed")?;

        Ok(())
    }

    pub async fn delay_proxy(&self, name: &str) -> Result<u64> {
        self.delay_proxy_with_options(name, DELAY_TEST_URL, 5000)
            .await
    }

    pub async fn delay_proxy_with_options(
        &self,
        name: &str,
        test_url: &str,
        timeout_ms: u64,
    ) -> Result<u64> {
        let mut url = Url::parse(&self.url("/")).context("failed to build clash api url")?;
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("failed to build proxy delay path"))?
            .extend(["proxies", name, "delay"]);
        url.query_pairs_mut()
            .append_pair("timeout", &timeout_ms.to_string())
            .append_pair("url", test_url);

        let response = self
            .http
            .get(url)
            .timeout(Duration::from_millis(timeout_ms).saturating_add(Duration::from_secs(3)))
            .send()
            .await
            .context("failed to request proxy delay")?
            .error_for_status()
            .context("proxy delay request failed")?
            .json::<DelayResponse>()
            .await
            .context("failed to parse proxy delay")?;

        if response.delay == 0 {
            bail!("proxy delay returned 0");
        }

        Ok(response.delay)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

#[derive(Debug, Deserialize)]
struct DelayResponse {
    delay: u64,
}
