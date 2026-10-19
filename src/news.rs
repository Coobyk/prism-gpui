//! News feed. Mirrors launcher/news (Prism Launcher RSS/JSON feed).

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct NewsEntry {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default, rename = "link")]
    pub url: String,
    #[serde(default)]
    pub date: String,
}

/// Fetch the Prism Launcher news feed.
pub fn fetch() -> Result<Vec<NewsEntry>, String> {
    let body = crate::net::get_string("https://prismlauncher.org/feed.xml")?;
    Ok(parse_rss(&body))
}

/// Minimal RSS parser (extracts <item> title/link/description/pubDate).
pub fn parse_rss(xml: &str) -> Vec<NewsEntry> {
    let mut entries = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<item>") {
        let after = &rest[start + "<item>".len()..];
        let Some(end) = after.find("</item>") else {
            break;
        };
        let item = &after[..end];
        entries.push(NewsEntry {
            title: tag(item, "title"),
            url: tag(item, "link"),
            content: strip_tags(&tag(item, "description")),
            date: tag(item, "pubDate"),
        });
        rest = &after[end + "</item>".len()..];
    }
    entries
}

fn tag(xml: &str, name: &str) -> String {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    // also handle CDATA
    if let Some(s) = xml.find(&open) {
        let after = &xml[s + open.len()..];
        if let Some(e) = after.find(&close) {
            return after[..e]
                .replace("<![CDATA[", "")
                .replace("]]>", "")
                .trim()
                .to_string();
        }
    }
    String::new()
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_items() {
        let xml = r#"<rss><channel>
            <item><title>Hello</title><link>https://x</link><description><![CDATA[<p>Body</p>]]></description><pubDate>Mon</pubDate></item>
            <item><title>Two</title></item>
        </channel></rss>"#;
        let items = parse_rss(xml);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Hello");
        assert_eq!(items[0].url, "https://x");
        assert_eq!(items[0].content, "Body");
        assert_eq!(items[1].title, "Two");
    }
}
