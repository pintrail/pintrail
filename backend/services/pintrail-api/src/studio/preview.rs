//! Link previews for artifact links: fetch a page and read its Open Graph /
//! `<title>` metadata, the way Notion or Slack unfurl a pasted URL.
//!
//! Best-effort by design. A page that is slow, huge, not HTML, or unreachable
//! simply yields no preview, and the Studio shows the bare hostname instead.
//! Nothing here can fail the request that adds the link.
//!
//! Only editors can trigger a fetch, but it still runs from inside the
//! server's network, so it refuses hosts that resolve to private, loopback,
//! or link-local addresses rather than becoming a probe of the internal net.

use std::net::IpAddr;
use std::time::Duration;

use reqwest::Url;

/// The most of a page we will read. Metadata lives in `<head>`, which is
/// almost always within the first few kilobytes.
const MAX_BYTES: usize = 512 * 1024;
const TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Preview {
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub site_name: Option<String>,
}

/// Normalises what an author typed into an absolute http(s) URL, adding
/// `https://` when they pasted a bare `example.com/page`.
pub fn normalize_url(raw: &str) -> Result<Url, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("Enter a web address.".into());
    }
    let with_scheme = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("https://{raw}")
    };
    let url = Url::parse(&with_scheme).map_err(|_| "That doesn't look like a web address.".to_string())?;
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err("Only http and https links are allowed.".into()),
    }
    match url.host_str() {
        Some(h) if h.contains('.') || h.parse::<IpAddr>().is_ok() => {}
        _ => return Err("That doesn't look like a web address.".into()),
    }
    if url.as_str().len() > 2000 {
        return Err("That web address is too long.".into());
    }
    Ok(url)
}

/// Fetches `url` and extracts its preview, or `None` if anything goes wrong.
pub async fn fetch(url: &Url) -> Option<Preview> {
    match fetch_inner(url).await {
        Ok(p) => Some(p),
        Err(e) => {
            tracing::info!(url = %url, error = %e, "link preview unavailable");
            None
        }
    }
}

async fn fetch_inner(url: &Url) -> Result<Preview, String> {
    ensure_public(url).await?;

    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.error("too many redirects");
            }
            let next = attempt.url();
            if !matches!(next.scheme(), "http" | "https") || literal_is_private(next) {
                return attempt.stop();
            }
            attempt.follow()
        }))
        .user_agent("Mozilla/5.0 (compatible; PintrailStudio/1.0; link preview)")
        .build()
        .map_err(|e| e.to_string())?;

    let mut resp = client
        .get(url.clone())
        .header("accept", "text/html,application/xhtml+xml;q=0.9,*/*;q=0.1")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("status {}", resp.status()));
    }
    let is_html = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.contains("html"))
        .unwrap_or(true);
    if !is_html {
        return Err("not an HTML page".into());
    }
    let final_url = resp.url().clone();

    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        body.extend_from_slice(&chunk);
        if body.len() >= MAX_BYTES || contains_ci(&body, b"</head>") {
            break;
        }
    }
    let html = String::from_utf8_lossy(&body);
    Ok(parse(&html, &final_url))
}

/// Rejects a URL whose host resolves to anything but public addresses.
async fn ensure_public(url: &Url) -> Result<(), String> {
    let host = url.host_str().ok_or("no host")?;
    if host.eq_ignore_ascii_case("localhost") {
        return Err("private host".into());
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let addrs = tokio::net::lookup_host((host.trim_matches(['[', ']']), port))
        .await
        .map_err(|e| e.to_string())?;
    let mut any = false;
    for addr in addrs {
        any = true;
        if !is_public_ip(addr.ip()) {
            return Err("private address".into());
        }
    }
    if any { Ok(()) } else { Err("no address".into()) }
}

fn literal_is_private(url: &Url) -> bool {
    match url.host_str() {
        Some(h) if h.eq_ignore_ascii_case("localhost") => true,
        Some(h) => h
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .map(|ip| !is_public_ip(ip))
            .unwrap_or(false),
        None => true,
    }
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || o[0] == 0
                || (o[0] == 100 && (o[1] & 0xc0) == 64)) // 100.64.0.0/10 CGNAT
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            let s = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || (s[0] & 0xfe00) == 0xfc00 // unique local
                || (s[0] & 0xffc0) == 0xfe80) // link local
        }
    }
}

fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle))
}

// --- HTML metadata extraction -------------------------------------------------
//
// A deliberately small scanner rather than a full HTML parser: we only need
// the attributes of <meta> and <link> tags and the text of <title>, all of
// which sit in <head>. Malformed markup degrades to "no preview".

/// Extracts a preview from an HTML document fetched from `base`.
pub fn parse(html: &str, base: &Url) -> Preview {
    let mut og_title = None;
    let mut og_desc = None;
    let mut og_image = None;
    let mut og_site = None;
    let mut tw_title = None;
    let mut tw_desc = None;
    let mut tw_image = None;
    let mut meta_desc = None;

    for attrs in tags(html, "meta") {
        let key = attr(&attrs, "property")
            .or_else(|| attr(&attrs, "name"))
            .map(|k| k.to_ascii_lowercase());
        let Some(key) = key else { continue };
        let Some(content) = attr(&attrs, "content").map(|c| clean(&c)).filter(|c| !c.is_empty()) else {
            continue;
        };
        let slot = match key.as_str() {
            "og:title" => &mut og_title,
            "og:description" => &mut og_desc,
            "og:image" | "og:image:url" | "og:image:secure_url" => &mut og_image,
            "og:site_name" => &mut og_site,
            "twitter:title" => &mut tw_title,
            "twitter:description" => &mut tw_desc,
            "twitter:image" | "twitter:image:src" => &mut tw_image,
            "description" => &mut meta_desc,
            _ => continue,
        };
        if slot.is_none() {
            *slot = Some(content);
        }
    }

    let title = og_title.or(tw_title).or_else(|| title_text(html));
    let description = og_desc.or(tw_desc).or(meta_desc).map(|d| truncate(&d, 300));
    let image_url = og_image
        .or(tw_image)
        .and_then(|src| base.join(&src).ok())
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .map(|u| u.to_string());

    Preview {
        title: title.map(|t| truncate(&t, 200)),
        description,
        image_url,
        site_name: og_site.map(|s| truncate(&s, 100)),
    }
}

/// Yields the raw attribute string of each `<name ...>` tag.
fn tags<'a>(html: &'a str, name: &'a str) -> impl Iterator<Item = String> + 'a {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{name}");
    let mut pos = 0;
    std::iter::from_fn(move || loop {
        let start = pos + lower[pos..].find(&open)?;
        let after = start + open.len();
        let next = lower[after..].chars().next()?;
        if !(next.is_whitespace() || next == '/' || next == '>') {
            pos = after;
            continue;
        }
        let end = after + lower[after..].find('>')?;
        pos = end + 1;
        return Some(html[after..end].to_string());
    })
}

/// Reads one attribute's value out of a tag's attribute string.
fn attr(attrs: &str, name: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let bytes = attrs.as_bytes();
    let mut search = 0;
    while let Some(rel) = lower[search..].find(name) {
        let i = search + rel;
        search = i + name.len();
        // Must be a whole attribute name.
        let before_ok = i == 0 || bytes[i - 1].is_ascii_whitespace() || bytes[i - 1] == b'/';
        let mut j = i + name.len();
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if !before_ok || j >= bytes.len() || bytes[j] != b'=' {
            continue;
        }
        j += 1;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() {
            return None;
        }
        let (start, end) = match bytes[j] {
            q @ (b'"' | b'\'') => {
                let s = j + 1;
                let e = attrs[s..].find(q as char).map(|r| s + r).unwrap_or(attrs.len());
                (s, e)
            }
            _ => {
                let e = attrs[j..]
                    .find(|c: char| c.is_whitespace() || c == '>')
                    .map(|r| j + r)
                    .unwrap_or(attrs.len());
                (j, e)
            }
        };
        return Some(attrs[start..end].to_string());
    }
    None
}

fn title_text(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let open = lower.find("<title")?;
    let start = open + lower[open..].find('>')? + 1;
    let end = start + lower[start..].find("</title")?;
    Some(clean(&html[start..end])).filter(|t| !t.is_empty())
}

/// Decodes the handful of entities that show up in titles and collapses
/// whitespace. Rendering escapes the result again, so this is display-only.
fn clean(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let Some(semi) = rest.as_bytes()[..rest.len().min(12)].iter().position(|&b| b == b';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..semi];
        let decoded = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" | "#x27" => Some('\''),
            "nbsp" => Some(' '),
            "ndash" => Some('–'),
            "mdash" => Some('—'),
            "rsquo" => Some('’'),
            "lsquo" => Some('‘'),
            "rdquo" => Some('”'),
            "ldquo" => Some('“'),
            "hellip" => Some('…'),
            _ => ent
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max - 1).collect();
    t.push('…');
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://www.umass.edu/sustainability/page").unwrap()
    }

    #[test]
    fn prefers_open_graph() {
        let html = r#"<html><head>
            <title>Fallback title</title>
            <meta name="description" content="plain description">
            <meta property="og:title" content="Solar at UMass &amp; Beyond">
            <meta property='og:description' content='The OG description'>
            <meta property="og:image" content="/img/solar.jpg">
            <meta property="og:site_name" content="UMass Amherst">
        </head></html>"#;
        let p = parse(html, &base());
        assert_eq!(p.title.as_deref(), Some("Solar at UMass & Beyond"));
        assert_eq!(p.description.as_deref(), Some("The OG description"));
        assert_eq!(p.image_url.as_deref(), Some("https://www.umass.edu/img/solar.jpg"));
        assert_eq!(p.site_name.as_deref(), Some("UMass Amherst"));
    }

    #[test]
    fn falls_back_to_title_and_meta_description() {
        let html = "<HEAD><TITLE>\n  Central   Heating Plant\n</TITLE>\
                    <META NAME=description CONTENT=\"Steam &#8211; and power\"></HEAD>";
        let p = parse(html, &base());
        assert_eq!(p.title.as_deref(), Some("Central Heating Plant"));
        assert_eq!(p.description.as_deref(), Some("Steam – and power"));
        assert_eq!(p.image_url, None);
    }

    #[test]
    fn ignores_metadata_lookalikes() {
        // <metadata> is not <meta>, and data-name is not name.
        let html = r#"<metadata name="description" content="no"><meta data-name="description" content="no"><title>T</title>"#;
        let p = parse(html, &base());
        assert_eq!(p.description, None);
        assert_eq!(p.title.as_deref(), Some("T"));
    }

    #[test]
    fn normalizes_urls() {
        assert_eq!(normalize_url("umass.edu/x").unwrap().as_str(), "https://umass.edu/x");
        assert!(normalize_url("javascript:alert(1)").is_err());
        assert!(normalize_url("ftp://umass.edu").is_err());
        assert!(normalize_url("notaurl").is_err());
        assert!(normalize_url("").is_err());
    }

    #[test]
    fn blocks_private_addresses() {
        for ip in ["127.0.0.1", "10.1.2.3", "192.168.0.5", "172.16.4.4", "169.254.169.254", "100.64.1.1", "::1", "fd00::1", "::ffff:10.0.0.1"] {
            assert!(!is_public_ip(ip.parse().unwrap()), "{ip} should be private");
        }
        assert!(is_public_ip("128.119.8.148".parse().unwrap()));
        assert!(literal_is_private(&Url::parse("http://169.254.169.254/latest").unwrap()));
        assert!(!literal_is_private(&Url::parse("https://www.umass.edu/").unwrap()));
    }
}
