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
    let body = crate::net::get_string("https://prismlauncher.org/feed/feed.xml")?;
    Ok(parse_rss(&body))
}

/// Minimal feed parser: supports RSS (`<item>`) and Atom (`<entry>`).
pub fn parse_rss(xml: &str) -> Vec<NewsEntry> {
    let mut entries = Vec::new();
    if xml.contains("<entry") {
        let mut rest = xml;
        while let Some(start) = rest.find("<entry") {
            let after_open = match rest[start..].find('>') {
                Some(i) => &rest[start + i + 1..],
                None => break,
            };
            let Some(end) = after_open.find("</entry>") else {
                break;
            };
            let item = &after_open[..end];
            entries.push(NewsEntry {
                title: strip_tags(&tag(item, "title")),
                url: atom_link(item),
                content: strip_tags(&first_tag(item, &["content", "summary"])),
                date: tag(item, "updated"),
            });
            rest = &after_open[end + "</entry>".len()..];
        }
        return entries;
    }
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

/// Extract the href of the first Atom `<link ... href="...">`.
fn atom_link(item: &str) -> String {
    let mut rest = item;
    while let Some(start) = rest.find("<link") {
        let after = &rest[start..];
        let end = after.find('>').unwrap_or(after.len());
        let tag = &after[..end];
        if let Some(h) = attr(tag, "href") {
            return h;
        }
        rest = &rest[start + 5..];
    }
    String::new()
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let start = tag.find(&key)? + key.len();
    let after = &tag[start..];
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

fn first_tag(item: &str, names: &[&str]) -> String {
    for n in names {
        let v = tag(item, n);
        if !v.is_empty() {
            return v;
        }
    }
    String::new()
}

fn tag(xml: &str, name: &str) -> String {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let Some(s) = xml.find(&open) else {
        return String::new();
    };
    // Require the character after the name to be '>' or whitespace.
    let after_name = &xml[s + open.len()..];
    match after_name.chars().next() {
        Some('>') => {}
        Some(c) if c.is_whitespace() => {}
        _ => return String::new(),
    }
    let Some(gt) = after_name.find('>') else {
        return String::new();
    };
    let body = &after_name[gt + 1..];
    let Some(e) = body.find(&close) else {
        return String::new();
    };
    body[..e]
        .replace("<![CDATA[", "")
        .replace("]]>", "")
        .trim()
        .to_string()
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
    fn parses_atom() {
        let xml = r#"<feed><entry>
            <title type="html">Hello &amp; Hi</title>
            <link rel="alternate" href="https://x/1"/>
            <content type="html"><![CDATA[<p>Body</p>]]></content>
            <updated>2026-01-01</updated>
        </entry></feed>"#;
        let items = parse_rss(xml);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Hello &amp; Hi");
        assert_eq!(items[0].url, "https://x/1");
        assert_eq!(items[0].content, "Body");
    }

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
