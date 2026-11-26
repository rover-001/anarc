//! HTTPS-first URL eligibility and host extraction helpers.

use std::sync::atomic::Ordering;

use super::PLUGIN_ENABLED;

pub fn url_host(url: &str) -> Option<String> {
    let (_, remainder) = url.split_once("://")?;
    let authority = remainder.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    let host = if authority.starts_with('[') {
        authority.split(']').next()?.trim_start_matches('[')
    } else {
        authority.split(':').next()?
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

pub fn eligible_for_https(url: &str) -> bool {
    let Some(host) = url_host(url) else {
        return false;
    };
    if !url.starts_with("http://") {
        return false;
    }
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return false;
    }
    host.parse::<std::net::IpAddr>().is_err()
}

pub fn upgrade_http_url(url: &str) -> Option<String> {
    (PLUGIN_ENABLED.load(Ordering::Relaxed) && eligible_for_https(url))
        .then(|| format!("https{}", &url[4..]))
}
