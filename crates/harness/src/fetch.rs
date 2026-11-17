use std::time::Duration;

use serde_json::Value;
use ureq::Agent;

const LIMIT: usize = 20_000;

pub fn host(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    Some(&rest[..end]).filter(|host| !host.is_empty())
}

pub fn get(input: &Value) -> Result<String, String> {
    let url = input["url"]
        .as_str()
        .filter(|url| host(url).is_some())
        .ok_or_else(|| "url must start with https:// or http://".to_string())?;
    let agent: Agent = Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    let mut response = agent
        .get(url)
        .header("User-Agent", "sombre")
        .call()
        .map_err(|error| format!("couldn't fetch {url}: {error}"))?;
    let status = response.status().as_u16();
    if (300..400).contains(&status) {
        let target = response
            .headers()
            .get("location")
            .and_then(|place| place.to_str().ok())
            .unwrap_or_default();
        return Ok(format!(
            "{url} redirects to {target}. fetch that address if it's the page you need"
        ));
    }
    let html = response
        .headers()
        .get("content-type")
        .and_then(|kind| kind.to_str().ok())
        .is_some_and(|kind| kind.contains("html"));
    let body = response
        .body_mut()
        .with_config()
        .limit(5 * 1024 * 1024)
        .read_to_string()
        .map_err(|error| format!("couldn't read {url}: {error}"))?;
    let text = if html { strip(&body) } else { body };
    let mut text: String = text.chars().take(LIMIT).collect();
    if status >= 400 {
        text = format!("{url} answered {status}\n{text}");
    }
    Ok(text)
}

pub fn strip(html: &str) -> String {
    let mut text = String::new();
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        text.push_str(&rest[..open]);
        let tag = &rest[open..];
        let lower = tag.get(..8).unwrap_or(tag).to_lowercase();
        let skip = ["<script", "<style", "<noscript"]
            .iter()
            .find(|name| lower.starts_with(**name))
            .map(|name| format!("</{}", &name[1..]));
        rest = match skip {
            Some(close) => tag
                .to_lowercase()
                .find(&close)
                .and_then(|end| tag[end..].find('>').map(|stop| &tag[end + stop + 1..]))
                .unwrap_or(""),
            None => {
                let block = [
                    "<p", "<br", "<div", "<li", "<h", "<tr", "</p", "</div", "</li", "</h",
                ]
                .iter()
                .any(|name| lower.starts_with(name));
                if block {
                    text.push('\n');
                }
                tag.find('>').map_or("", |end| &tag[end + 1..])
            }
        };
    }
    text.push_str(rest);
    let text = text
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_host() {
        assert_eq!(host("https://sombre.ai/docs?x=1"), Some("sombre.ai"));
        assert_eq!(host("http://localhost:3000"), Some("localhost:3000"));
        assert_eq!(host("ftp://nope"), None);
        assert_eq!(host("https://"), None);
    }

    #[test]
    fn turns_html_into_readable_text() {
        let html = "<html><head><style>p{color:red}</style><script>alert(1)</script></head><body><h1>Title</h1><p>Hello &amp; welcome</p><div>second   line</div></body></html>";
        assert_eq!(strip(html), "Title\nHello & welcome\nsecond line");
    }

    #[test]
    fn rejects_urls_without_a_scheme() {
        assert!(get(&serde_json::json!({"url": "sombre.ai"})).is_err());
    }
}
