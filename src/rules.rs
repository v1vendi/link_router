//! Rules file parsing and URL matching (format documented in rules.example).
//! No AppKit here, so it is unit-testable.

pub enum Kind {
    Opener(String),
    Domain(String),
    Prefix(String),
}

pub struct Rule {
    pub profile: String,
    pub kind: Kind,
}

pub fn parse(text: &str) -> Vec<Rule> {
    let mut rules = Vec::new();
    let mut profile: Option<String> = None;
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            profile = Some(name.trim().to_string());
            continue;
        }
        let (Some(profile), Some((kind, value))) = (&profile, line.split_once(char::is_whitespace))
        else {
            eprintln!("linkrouter: ignoring rule line: {raw}");
            continue;
        };
        let v = value.trim().to_ascii_lowercase();
        let kind = match kind {
            "opener" => Kind::Opener(v),
            "domain" => Kind::Domain(v),
            "prefix" => Kind::Prefix(strip_scheme_www(&v).trim_end_matches('/').to_string()),
            _ => {
                eprintln!("linkrouter: unknown rule kind: {raw}");
                continue;
            }
        };
        rules.push(Rule {
            profile: profile.clone(),
            kind,
        });
    }
    rules
}

fn strip_scheme_www(s: &str) -> &str {
    let s = s.split_once("://").map_or(s, |(_, rest)| rest);
    s.strip_prefix("www.").unwrap_or(s)
}

/// For http(s) URLs: (lowercase host, lowercase "host-without-www + path").
fn split_web_url(url: &str) -> Option<(String, String)> {
    let (scheme, rest) = url.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = rest[..end].rsplit('@').next().unwrap_or("");
    let host = if authority.starts_with('[') {
        authority.split(']').next().unwrap_or("").to_string() + "]"
    } else {
        authority.split(':').next().unwrap_or("").to_string()
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let rest_lc = format!("{}{}", strip_scheme_www(&host), &rest[end..]).to_ascii_lowercase();
    Some((host, rest_lc))
}

/// Returns the Chrome profile directory to use, or None for the default browser behaviour.
pub fn route<'a>(rules: &'a [Rule], url: &str, opener: Option<&str>) -> Option<&'a str> {
    let web = split_web_url(url);
    rules
        .iter()
        .find(|r| match &r.kind {
            Kind::Opener(v) => opener.is_some_and(|o| o.eq_ignore_ascii_case(v)),
            Kind::Domain(v) => web.as_ref().is_some_and(|(host, _)| {
                host.strip_suffix(v.as_str())
                    .is_some_and(|sub| sub.is_empty() || sub.ends_with('.'))
            }),
            Kind::Prefix(v) => web.as_ref().is_some_and(|(_, s)| {
                s.strip_prefix(v.as_str())
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?', '#']))
            }),
        })
        .map(|r| r.profile.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: &str = "
        # work
        [Profile 3]
        opener com.tinyspeck.slackmacgap
        domain acme.io
        prefix https://www.github.com/acme-corp/
    ";

    fn r(url: &str, opener: Option<&str>) -> Option<String> {
        route(&parse(RULES), url, opener).map(str::to_string)
    }

    #[test]
    fn matches() {
        let w = Some("Profile 3".to_string());
        assert_eq!(r("https://github.com/acme-corp", None), w);
        assert_eq!(r("https://github.com/acme-corp/bot/pull/1", None), w);
        assert_eq!(r("https://www.GitHub.com/Acme-Corp?tab=repos", None), w);
        assert_eq!(r("https://acme.io/", None), w);
        assert_eq!(r("https://dev.api.acme.io:8443/x", None), w);
        assert_eq!(
            r("https://example.com", Some("com.tinyspeck.slackmacgap")),
            w
        );
    }

    #[test]
    fn non_matches() {
        assert_eq!(r("https://github.com/acme-corpno", None), None);
        assert_eq!(r("https://github.com/other/acme-corp", None), None);
        assert_eq!(r("https://notacme.io/", None), None);
        assert_eq!(r("https://acme.io.evil.com/", None), None);
        assert_eq!(r("https://evil.com/?u=https://acme.io", None), None);
        assert_eq!(r("https://user@evil.com/acme.io", None), None);
        assert_eq!(r("https://example.com", Some("com.apple.mail")), None);
        assert_eq!(r("file:///tmp/acme.io.html", None), None);
    }
}
