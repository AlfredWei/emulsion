//! Where the map tiles and the Nominatim place search come from.
//!
//! Both default to OpenStreetMap's public servers, which are fine for personal use but carry usage policies
//! that say access "may be withdrawn at any point", ask apps not to hard-code the tile URL, and allow at most
//! one search request per second across all users. So the two addresses (and the attribution a different tile
//! provider requires) are settings, validated here. Everything in this module is pure: no network, no catalog.
//! A value that does not pass is rejected when it is saved and ignored (the default is used) when read.

use reqwest::Url;

pub const DEFAULT_TILE_URL: &str = "https://tile.openstreetmap.org/{z}/{x}/{y}.png";
pub const DEFAULT_NOMINATIM_BASE: &str = "https://nominatim.openstreetmap.org";
const MAX_ATTRIBUTION_CHARS: usize = 200;

/// A web address that is safe to hand the map or the HTTP client: `https`, or `http` only for a server on this
/// computer (a self-hosted tile or Nominatim server during development); a host; no embedded credentials; no
/// fragment.
fn parse_web_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw.trim()).map_err(|_| "not a valid web address".to_string())?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    match url.scheme() {
        "https" => {}
        "http" if local => {}
        _ => return Err("must start with https:// (http:// is accepted only for localhost)".to_string()),
    }
    if url.host_str().is_none() {
        return Err("has no host name".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("must not contain a user name or password".to_string());
    }
    if url.fragment().is_some() {
        return Err("must not contain a # fragment".to_string());
    }
    Ok(url)
}

/// A tile address template: a web address containing `{z}`, `{x}` and `{y}`.
pub fn clean_tile_url(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    for placeholder in ["{z}", "{x}", "{y}"] {
        if !raw.contains(placeholder) {
            return Err(format!("the tile address needs {placeholder} in it"));
        }
    }
    parse_web_url(raw)?;
    Ok(raw.to_string())
}

/// The base address of a Nominatim server (the part before `/search` and `/reverse`), without a trailing slash.
pub fn clean_nominatim_base(raw: &str) -> Result<String, String> {
    let url = parse_web_url(raw)?;
    if url.query().is_some() {
        return Err("must not contain a ? query".to_string());
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

/// The credit a tile provider asks for, shown as plain text on the map.
pub fn clean_attribution(raw: &str) -> Result<String, String> {
    let text = raw.trim();
    if text.is_empty() {
        return Err("is empty".to_string());
    }
    if text.chars().count() > MAX_ATTRIBUTION_CHARS {
        return Err(format!("is longer than {MAX_ATTRIBUTION_CHARS} characters"));
    }
    if text.chars().any(char::is_control) {
        return Err("must be a single line of plain text".to_string());
    }
    Ok(text.to_string())
}

/// The search and reverse-lookup addresses for a stored base address; the OpenStreetMap server when none is set
/// or the stored value no longer passes validation.
pub fn nominatim_urls(stored_base: Option<&str>) -> (String, String) {
    let base = stored_base.and_then(|b| clean_nominatim_base(b).ok()).unwrap_or_else(|| DEFAULT_NOMINATIM_BASE.to_string());
    (format!("{base}/search"), format!("{base}/reverse"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_pass_their_own_validation() {
        assert_eq!(clean_tile_url(DEFAULT_TILE_URL).as_deref(), Ok(DEFAULT_TILE_URL));
        assert_eq!(clean_nominatim_base(DEFAULT_NOMINATIM_BASE).as_deref(), Ok(DEFAULT_NOMINATIM_BASE));
    }

    #[test]
    fn a_tile_address_needs_all_three_placeholders_and_a_safe_scheme() {
        assert!(clean_tile_url("https://tiles.example.com/{z}/{x}/{y}.png?key=abc").is_ok());
        assert!(clean_tile_url("  https://tiles.example.com/{z}/{x}/{y}.png  ").is_ok());
        assert!(clean_tile_url("https://tiles.example.com/{z}/{x}.png").is_err(), "no {{y}}");
        assert!(clean_tile_url("http://tiles.example.com/{z}/{x}/{y}.png").is_err(), "plain http to a remote host");
        assert!(clean_tile_url("http://localhost:8080/{z}/{x}/{y}.png").is_ok(), "plain http to this computer");
        assert!(clean_tile_url("http://127.0.0.1:8080/{z}/{x}/{y}.png").is_ok());
        assert!(clean_tile_url("file:///etc/passwd/{z}/{x}/{y}").is_err());
        assert!(clean_tile_url("javascript:alert(1)//{z}/{x}/{y}").is_err());
        assert!(clean_tile_url("https://user:pw@tiles.example.com/{z}/{x}/{y}.png").is_err(), "credentials in the address");
        assert!(clean_tile_url("https://tiles.example.com/{z}/{x}/{y}.png#frag").is_err());
        assert!(clean_tile_url("").is_err());
    }

    #[test]
    fn a_nominatim_base_is_trimmed_and_has_no_trailing_slash_or_query() {
        assert_eq!(clean_nominatim_base(" https://geo.example.com/ ").as_deref(), Ok("https://geo.example.com"));
        assert_eq!(clean_nominatim_base("https://geo.example.com/nominatim/").as_deref(), Ok("https://geo.example.com/nominatim"));
        assert!(clean_nominatim_base("https://geo.example.com/?key=1").is_err());
        assert!(clean_nominatim_base("ftp://geo.example.com").is_err());
        assert!(clean_nominatim_base("http://geo.example.com").is_err());
        assert!(clean_nominatim_base("http://localhost:8088").is_ok());
        assert!(clean_nominatim_base("not a url").is_err());
    }

    #[test]
    fn attribution_is_one_short_line_of_text() {
        assert_eq!(clean_attribution("  © Example Maps  ").as_deref(), Ok("© Example Maps"));
        assert!(clean_attribution("   ").is_err());
        assert!(clean_attribution("two\nlines").is_err());
        assert!(clean_attribution(&"x".repeat(MAX_ATTRIBUTION_CHARS + 1)).is_err());
        assert!(clean_attribution(&"x".repeat(MAX_ATTRIBUTION_CHARS)).is_ok());
    }

    #[test]
    fn nominatim_addresses_fall_back_to_the_default_when_unset_or_invalid() {
        let default = ("https://nominatim.openstreetmap.org/search".to_string(), "https://nominatim.openstreetmap.org/reverse".to_string());
        assert_eq!(nominatim_urls(None), default);
        assert_eq!(nominatim_urls(Some("not a url")), default, "a stored value that no longer validates is ignored");
        assert_eq!(
            nominatim_urls(Some("https://geo.example.com/nominatim")),
            ("https://geo.example.com/nominatim/search".to_string(), "https://geo.example.com/nominatim/reverse".to_string())
        );
    }
}
