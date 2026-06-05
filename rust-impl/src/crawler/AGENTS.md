<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-24 | Updated: 2026-05-24 -->

# crawler

## Purpose
文档爬取模块，提供 Chromium（无头浏览器）和 Reqwest（HTTP 客户端）双引擎爬虫，用于从向量数据库官方文档网站爬取页面内容并提取结构化信息（TOC、内容链接、正文）。

## Key Files
| File | Description |
|------|-------------|
| `mod.rs` | 模块声明 + 公共导出：ChromiumCrawler、ReqwestCrawler、Crawler trait、crawl_docs_site |
| `engine.rs` | 爬虫引擎实现：ChromiumCrawler（JS 渲染）、ReqwestCrawler（静态抓取）、crawl_docs_site（BFS 递归爬取） |
| `parser.rs` | HTML 解析器：clean_content（清理 Markdown）、extract_content_links（提取页面内链）、extract_toc（提取目录结构） |

## Subdirectories
（无子目录）

## For AI Agents

### Working In This Directory
- 默认使用 ChromiumCrawler，不可用时自动降级到 ReqwestCrawler
- `crawl_docs_site()` 实现 BFS 递归爬取，限制：同域、同路径前缀、深度 ≤ 3、上限 50 页
- 爬取结果持久化为 `contracts/{target}_crawled_pages.json`
- 修改爬取策略：调整 `engine.rs` 中的 BFS 参数

### Testing Requirements
- 修改爬虫后需手动验证爬取页面数和内容质量
- 增量模式：重复运行时跳过已爬页面

### Common Patterns
- Crawler trait：`async fn crawl(&self, url: &str) -> Result<CrawledPage>`
- CrawledPage 结构：`{ url, title, content_md, links }`
- 降级策略：Chromium 不可用 → warn 日志 + Reqwest 降级

## Dependencies

### Internal
- `contract_loader.rs`（调用爬虫获取文档内容）

### External
- chromiumoxide（Chromium 无头浏览器）
- reqwest（HTTP 客户端）
- scraper（HTML 解析）
- html2md（HTML 转 Markdown）
