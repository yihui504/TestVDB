use regex::Regex;
use scraper::{Html, Selector};
use std::sync::LazyLock;

static UNWANTED_TAGS_RE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    let tags = ["script", "style", "nav", "footer", "aside", "header"];
    tags.iter()
        .map(|tag| Regex::new(&format!(r"(?is)<{tag}[^>]*>.*?</{tag}>")).unwrap())
        .collect()
});

/// Extracts the Table of Contents (TOC) links from an HTML document.
/// It targets common sidebar/navigation elements like `<nav>`, `<aside>`, or `.menu`.
pub fn extract_toc(html: &str) -> Vec<String> {
    let document = Html::parse_document(html);
    
    // Selectors for common sidebar/nav areas
    let nav_selector = Selector::parse("nav, aside, .menu, .sidebar").unwrap();
    let link_selector = Selector::parse("a[href]").unwrap();

    let mut links = Vec::new();

    // Iterate over matching nav containers
    for nav_area in document.select(&nav_selector) {
        for link in nav_area.select(&link_selector) {
            if let Some(href) = link.value().attr("href") {
                // Ignore anchor links within the same page
                if !href.starts_with('#') {
                    links.push(href.to_string());
                }
            }
        }
    }

    // Deduplicate and return
    links.dedup();
    links
}

/// Extracts links from the main content area (article/main/.content), complementing `extract_toc`
/// which only picks up nav/sidebar links. Used for BFS crawl expansion.
pub fn extract_content_links(html: &str) -> Vec<String> {
    let document = Html::parse_document(html);
    let content_selector =
        Selector::parse("main a[href], article a[href], .content a[href], .document a[href]")
            .unwrap();

    let mut links = Vec::new();
    for link in document.select(&content_selector) {
        if let Some(href) = link.value().attr("href") {
            if !href.starts_with('#')
                && !href.starts_with("javascript:")
                && !href.starts_with("mailto:")
            {
                links.push(href.to_string());
            }
        }
    }
    links.dedup();
    links
}

/// Cleans the HTML content by stripping scripts, styles, navs, and footers,
/// then converts the core content into Markdown.
pub fn clean_content(html: &str) -> String {
    // 1. Remove unwanted tags using basic string manipulation / regex before parsing
    // (since scraper is primarily read-only, we do a pre-pass to strip noisy elements)
    let mut cleaned_html = html.to_string();
    
    for re in UNWANTED_TAGS_RE.iter() {
        cleaned_html = re.replace_all(&cleaned_html, "").to_string();
    }

    // 2. Try to isolate the main content
    let document = Html::parse_document(&cleaned_html);
    let main_selector = Selector::parse("main, article, .content, .document").unwrap();
    
    let target_html = if let Some(main_node) = document.select(&main_selector).next() {
        main_node.html()
    } else {
        cleaned_html
    };

    // Convert the isolated HTML to Markdown
    html2md::parse_html(&target_html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_toc() {
        let mock_html = r##"
            <html>
                <body>
                    <aside class="sidebar">
                        <a href="/docs/intro">Intro</a>
                        <a href="/docs/api#section">API</a>
                        <a href="#top">Top</a>
                    </aside>
                    <main>
                        <a href="/out">Out</a>
                    </main>
                </body>
            </html>
        "##;

        let toc = extract_toc(mock_html);
        assert_eq!(toc.len(), 2);
        assert_eq!(toc[0], "/docs/intro");
        assert_eq!(toc[1], "/docs/api#section");
    }

    #[test]
    fn test_clean_content() {
        let mock_html = r#"
            <html>
                <body>
                    <nav>Menu</nav>
                    <article>
                        <h1>Title</h1>
                        <p>This is the <strong>core</strong> content.</p>
                    </article>
                    <footer>Footer</footer>
                </body>
            </html>
        "#;

        let markdown = clean_content(mock_html);
        assert!(markdown.contains("Title\n=="));
        assert!(markdown.contains("This is the **core** content."));
        assert!(!markdown.contains("Menu"));
        assert!(!markdown.contains("Footer"));
    }

    #[test]
    fn test_clean_content_with_noisy_tags() {
        let mock_html = r#"
            <html>
                <body>
                    <header>Header Info</header>
                    <nav>Menu</nav>
                    <aside>Related Links</aside>
                    <div>
                        <h1>Fallback Title</h1>
                        <p>Fallback <strong>core</strong> content.</p>
                        <script>alert("noisy");</script>
                        <style>body { color: red; }</style>
                    </div>
                    <footer>Footer</footer>
                </body>
            </html>
        "#;

        let markdown = clean_content(mock_html);
        assert!(markdown.contains("Fallback Title\n=="));
        assert!(markdown.contains("Fallback **core** content."));
        // Should be explicitly stripped even without <main>
        assert!(!markdown.contains("Menu"));
        assert!(!markdown.contains("Footer"));
        assert!(!markdown.contains("Header Info"));
        assert!(!markdown.contains("Related Links"));
        assert!(!markdown.contains("alert(\"noisy\");"));
    }
}
