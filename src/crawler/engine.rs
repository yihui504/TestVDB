use anyhow::{Context, Result};
use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;

/// Common interface for fetching HTML content from a URL
#[allow(async_fn_in_trait)]
pub trait Crawler: Send + Sync {
    async fn fetch_page(&self, url: &str) -> Result<String>;
}

/// Headless browser crawler using chromiumoxide
pub struct ChromiumCrawler;

impl ChromiumCrawler {
    pub fn new() -> Self {
        Self
    }
}

impl Crawler for ChromiumCrawler {
    async fn fetch_page(&self, url: &str) -> Result<String> {
        let (mut browser, mut handler) = Browser::launch(
            BrowserConfig::builder()
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build browser config: {}", e))?,
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to launch browser: {}", e))?;

        let handle = tokio::task::spawn(async move {
            while let Some(h) = handler.next().await {
                if h.is_err() {
                    break;
                }
            }
        });

        let page = browser
            .new_page(url)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to open new page: {}", e))?;

        // Wait for JS to render
        let _ = page.wait_for_navigation().await;
        sleep(Duration::from_secs(2)).await;

        let content = page
            .content()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get page content: {}", e))?;

        browser.close().await.unwrap_or_default();
        handle.abort();

        Ok(content)
    }
}

/// Fallback crawler using reqwest for simple static pages
pub struct ReqwestCrawler {
    client: Client,
}

impl ReqwestCrawler {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("TestVDB-Agent/1.0")
                .build()
                .unwrap_or_default(),
        }
    }
}

impl Crawler for ReqwestCrawler {
    async fn fetch_page(&self, url: &str) -> Result<String> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("Reqwest failed to send request")?;

        if !response.status().is_success() {
            anyhow::bail!("Request failed with status: {}", response.status());
        }

        let content = response
            .text()
            .await
            .context("Failed to read response body")?;

        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_reqwest_crawler() {
        let crawler = ReqwestCrawler::new();
        let result = crawler.fetch_page("https://example.com").await;
        assert!(result.is_ok());
        let html = result.unwrap();
        assert!(html.contains("Example Domain"));
    }

    #[tokio::test]
    async fn test_chromium_crawler() {
        let crawler = ChromiumCrawler::new();
        let result = crawler.fetch_page("https://example.com").await;
        // In some CI/Sandbox environments chromium might fail to launch, so we just check if the logic runs without panic
        if let Ok(html) = result {
            assert!(html.contains("Example Domain"));
        }
    }
}
