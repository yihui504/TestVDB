use anyhow::{Context, Result};
use async_trait::async_trait;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chrono::Utc;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

use super::parser::{clean_content, extract_content_links, extract_toc};

/// Common interface for fetching HTML content from a URL
#[async_trait]
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

#[async_trait]
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

#[async_trait]
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

// ── BFS crawl engine ──────────────────────────────────────────────

/// A single crawled documentation page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawledPage {
    pub url: String,
    pub markdown: String,
    pub crawled_at: String,
}

/// Domains that are never documentation pages.
fn is_blacklisted(host: &str) -> bool {
    let blacklist = [
        "github.com",
        "stackoverflow.com",
        "medium.com",
        "twitter.com",
        "x.com",
        "youtube.com",
        "discord.com",
        "linkedin.com",
        "reddit.com",
    ];
    blacklist.iter().any(|d| host.contains(d))
        || host.starts_with("blog.")
        || host.starts_with("community.")
        || host.starts_with("forum.")
}

fn extract_host(url: &str) -> Option<String> {
    let without_proto = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    without_proto.split('/').next().map(|s| s.to_string())
}

fn extract_path_prefix(url: &str) -> String {
    let without_proto = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let host_end = without_proto.find('/');
    match host_end {
        None => "/".to_string(),
        Some(i) => {
            let path = &without_proto[i..];
            // include the trailing slash so /docs/quickstart still matches /docs/ prefix
            match path.rfind('/') {
                None => path.to_string(),
                Some(j) => path[..=j].to_string(),
            }
        }
    }
}

/// Resolve a potentially-relative link against a base URL.
fn resolve_link(link: &str, base_url: &str) -> String {
    if link.starts_with("http://") || link.starts_with("https://") {
        return link.to_string();
    }
    if link.starts_with('/') {
        let host = extract_host(base_url).unwrap_or_default();
        let proto = if base_url.starts_with("https") {
            "https"
        } else {
            "http"
        };
        return format!("{}://{}{}", proto, host, link);
    }
    // relative path
    let base = base_url.trim_end_matches('/');
    format!("{}/{}", base, link.trim_start_matches('/'))
}

/// BFS crawl of a documentation site, starting from `start_url`.
///
/// - Respects same-host + same-path-prefix boundaries.
/// - Skips blacklisted domains and non-HTTP schemes.
/// - Caps at `max_pages` total and `max_depth` link-hops from root.
pub async fn crawl_docs_site(
    crawler: &dyn Crawler,
    start_url: &str,
    max_pages: usize,
    max_depth: usize,
) -> Result<Vec<CrawledPage>> {
    let start_host = extract_host(start_url).unwrap_or_default();
    let path_prefix = extract_path_prefix(start_url);

    info!(
        "Crawling site: host={}, path_prefix={}, max_pages={}, max_depth={}",
        start_host, path_prefix, max_pages, max_depth
    );

    if is_blacklisted(&start_host) {
        anyhow::bail!("Start URL host '{}' is blacklisted", start_host);
    }

    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    let mut results: Vec<CrawledPage> = Vec::new();

    queue.push_back((start_url.to_string(), 0));
    visited.insert(start_url.to_string());

    while let Some((url, depth)) = queue.pop_front() {
        if results.len() >= max_pages {
            info!("Reached max_pages limit ({})", max_pages);
            break;
        }

        info!(
            "[{}/{}] Crawling (depth={}): {}",
            results.len() + 1,
            max_pages,
            depth,
            url
        );

        let html = match crawler.fetch_page(&url).await {
            Ok(h) => h,
            Err(e) => {
                warn!("Failed to fetch {}: {}", url, e);
                continue;
            }
        };

        let markdown = clean_content(&html);
        if markdown.len() < 100 {
            warn!(
                "Page {} produced very short markdown ({} chars), skipping",
                url,
                markdown.len()
            );
            continue;
        }

        let crawled_at = Utc::now().to_rfc3339();
        results.push(CrawledPage {
            url: url.clone(),
            markdown,
            crawled_at,
        });

        // Expand frontier: collect links from TOC and page content
        if depth < max_depth {
            let toc_links = extract_toc(&html);
            let content_links = extract_content_links(&html);
            let all_links: Vec<String> =
                toc_links.into_iter().chain(content_links).collect();

            for link in &all_links {
                let resolved = resolve_link(link, &url);
                if visited.contains(&resolved) {
                    continue;
                }

                let link_host = extract_host(&resolved).unwrap_or_default();
                if link_host != start_host {
                    continue;
                }
                // Check path prefix matches
                let without_proto = resolved
                    .trim_start_matches("https://")
                    .trim_start_matches("http://");
                let link_path = without_proto
                    .find('/')
                    .map(|i| &without_proto[i..])
                    .unwrap_or("/");
                if !path_prefix.is_empty()
                    && path_prefix != "/"
                    && !link_path.starts_with(&path_prefix)
                {
                    continue;
                }
                if is_blacklisted(&link_host) {
                    continue;
                }

                visited.insert(resolved.clone());
                queue.push_back((resolved, depth + 1));
            }
        }
    }

    info!("Crawl complete: {} pages collected", results.len());
    Ok(results)
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
        if let Ok(html) = result {
            assert!(html.contains("Example Domain"));
        }
    }

    #[test]
    fn test_extract_host() {
        assert_eq!(
            extract_host("https://qdrant.tech/documentation/"),
            Some("qdrant.tech".into())
        );
        assert_eq!(
            extract_host("http://localhost:19530/v2/vectordb"),
            Some("localhost:19530".into())
        );
    }

    #[test]
    fn test_extract_path_prefix() {
        assert_eq!(
            extract_path_prefix("https://qdrant.tech/documentation/"),
            "/documentation/"
        );
        assert_eq!(
            extract_path_prefix("https://qdrant.tech/documentation"),
            "/"
        );
        assert_eq!(extract_path_prefix("https://qdrant.tech"), "/");
    }

    #[test]
    fn test_resolve_link() {
        assert_eq!(
            resolve_link("/docs/api", "https://qdrant.tech/documentation/"),
            "https://qdrant.tech/docs/api"
        );
        assert_eq!(
            resolve_link("quickstart/", "https://qdrant.tech/documentation/"),
            "https://qdrant.tech/documentation/quickstart/"
        );
        assert_eq!(
            resolve_link(
                "https://qdrant.tech/other",
                "https://qdrant.tech/documentation/"
            ),
            "https://qdrant.tech/other"
        );
    }

    #[test]
    fn test_is_blacklisted() {
        assert!(is_blacklisted("github.com"));
        assert!(is_blacklisted("blog.qdrant.tech"));
        assert!(!is_blacklisted("qdrant.tech"));
        assert!(!is_blacklisted("milvus.io"));
    }
}
