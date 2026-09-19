//! Forward geocoding (RFC-0007 §3.2, as revised by its 2026-09-19 update).
//!
//! Two providers: OpenStreetMap's Nominatim (the default -- no key, no
//! account) and Google's Geocoding API (optional, the user's own key,
//! better at landmark/business names). The only thing sent off-device is
//! the user's typed search string (plus the key for Google), and only when
//! they press Search. Results are returned to the caller and never
//! persisted here; the caller persists only coordinates the user picks.
//!
//! Response parsing is pure so it is tested against recorded JSON bodies --
//! CI never touches the network.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const GOOGLE_ENDPOINT: &str = "https://maps.googleapis.com/maps/api/geocode/json";
const NOMINATIM_ENDPOINT: &str = "https://nominatim.openstreetmap.org/search";
const MAX_CANDIDATES: usize = 5;
// Nominatim's usage policy: an identifying User-Agent and at most one
// request per second.
const USER_AGENT: &str = "Emulsion/0.1 (+https://github.com/AlfredWei/emulsion)";
const NOMINATIM_MIN_INTERVAL: Duration = Duration::from_millis(1100);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Osm,
    Google,
}

impl Provider {
    pub fn as_setting(self) -> &'static str {
        match self {
            Provider::Osm => "osm",
            Provider::Google => "google",
        }
    }

    /// Unknown or missing values fall back to the keyless default.
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("google") => Provider::Google,
            _ => Provider::Osm,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeocodeCandidate {
    pub label: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum GeocodeError {
    #[error("Enter a place or address to search for.")]
    EmptyQuery,
    #[error("Google search is selected but no API key is set. Add one in Settings → Map, or switch to OpenStreetMap.")]
    NoApiKey,
    /// Transport failure. The URL is stripped from the underlying error
    /// before it gets here: it contains the API key.
    #[error("Couldn't reach the search service: {0}")]
    Network(String),
    #[error("The search service returned an unreadable response.")]
    BadResponse,
    #[error("The search service rejected the request: {0}")]
    Rejected(String),
    #[error("Search usage limit reached. Try again later or check your quota.")]
    OverQuota,
}

#[derive(Deserialize)]
struct Response {
    status: String,
    #[serde(default)]
    error_message: Option<String>,
    #[serde(default)]
    results: Vec<ResultItem>,
}

#[derive(Deserialize)]
struct ResultItem {
    formatted_address: String,
    geometry: Geometry,
}

#[derive(Deserialize)]
struct Geometry {
    location: LatLng,
}

#[derive(Deserialize)]
struct LatLng {
    lat: f64,
    lng: f64,
}

/// `ZERO_RESULTS` is a normal outcome, not an error: an empty list.
pub fn parse_google_response(body: &str) -> Result<Vec<GeocodeCandidate>, GeocodeError> {
    let response: Response = serde_json::from_str(body).map_err(|_| GeocodeError::BadResponse)?;
    match response.status.as_str() {
        "OK" => Ok(response
            .results
            .into_iter()
            .take(MAX_CANDIDATES)
            .map(|r| GeocodeCandidate {
                label: r.formatted_address,
                latitude: r.geometry.location.lat,
                longitude: r.geometry.location.lng,
            })
            .collect()),
        "ZERO_RESULTS" => Ok(Vec::new()),
        "OVER_QUERY_LIMIT" | "OVER_DAILY_LIMIT" => Err(GeocodeError::OverQuota),
        other => Err(GeocodeError::Rejected(
            response.error_message.unwrap_or_else(|| other.to_string()),
        )),
    }
}

#[derive(Deserialize)]
struct OsmItem {
    display_name: String,
    lat: String,
    lon: String,
}

/// Nominatim returns coordinates as strings. An item that doesn't parse is
/// skipped rather than failing the whole search.
pub fn parse_osm_response(body: &str) -> Result<Vec<GeocodeCandidate>, GeocodeError> {
    let items: Vec<OsmItem> = serde_json::from_str(body).map_err(|_| GeocodeError::BadResponse)?;
    Ok(items
        .into_iter()
        .filter_map(|i| {
            Some(GeocodeCandidate {
                latitude: i.lat.parse().ok()?,
                longitude: i.lon.parse().ok()?,
                label: i.display_name,
            })
        })
        .take(MAX_CANDIDATES)
        .collect())
}

static LAST_NOMINATIM_SLOT: Mutex<Option<Instant>> = Mutex::new(None);

/// How long a request arriving at `now` must wait so consecutive requests
/// start at least `min_interval` apart. `previous_slot` is when the
/// previous request was scheduled to start.
fn wait_for_slot(previous_slot: Option<Instant>, now: Instant, min_interval: Duration) -> Duration {
    match previous_slot {
        Some(prev) => (prev + min_interval).saturating_duration_since(now),
        None => Duration::ZERO,
    }
}

/// Atomically claims the next Nominatim slot and returns how long to wait
/// before using it.
fn claim_nominatim_slot() -> Duration {
    let now = Instant::now();
    let mut last = LAST_NOMINATIM_SLOT.lock().unwrap_or_else(|e| e.into_inner());
    let wait = wait_for_slot(*last, now, NOMINATIM_MIN_INTERVAL);
    *last = Some(now + wait);
    wait
}

fn client() -> Result<reqwest::Client, GeocodeError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| GeocodeError::Network(e.without_url().to_string()))
}

async fn fetch(request: reqwest::RequestBuilder) -> Result<String, GeocodeError> {
    request
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| GeocodeError::Network(e.without_url().to_string()))?
        .text()
        .await
        .map_err(|e| GeocodeError::Network(e.without_url().to_string()))
}

pub async fn search(
    provider: Provider,
    query: &str,
    google_api_key: Option<&str>,
) -> Result<Vec<GeocodeCandidate>, GeocodeError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(GeocodeError::EmptyQuery);
    }
    match provider {
        Provider::Google => {
            let key = google_api_key.filter(|k| !k.is_empty()).ok_or(GeocodeError::NoApiKey)?;
            let body = fetch(client()?.get(GOOGLE_ENDPOINT).query(&[("address", query), ("key", key)])).await?;
            parse_google_response(&body)
        }
        Provider::Osm => {
            let wait = claim_nominatim_slot();
            if !wait.is_zero() {
                let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(wait)).await;
            }
            let body = fetch(client()?.get(NOMINATIM_ENDPOINT).query(&[
                ("q", query),
                ("format", "jsonv2"),
                ("limit", "5"),
            ]))
            .await?;
            parse_osm_response(&body)
        }
    }
}

pub fn validate_coordinates(latitude: f64, longitude: f64) -> Result<(), String> {
    if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
        return Err(format!("Coordinates out of range: {latitude}, {longitude}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK_BODY: &str = r#"{
        "status": "OK",
        "results": [
            {"formatted_address": "Taipei 101, No. 7, Section 5, Xinyi Rd, Taiwan",
             "geometry": {"location": {"lat": 25.0339639, "lng": 121.5644722}, "location_type": "ROOFTOP"}},
            {"formatted_address": "Taipei, Taiwan",
             "geometry": {"location": {"lat": 25.0329694, "lng": 121.5654177}}}
        ]
    }"#;

    #[test]
    fn parses_candidates_in_order_with_coordinates() {
        let got = parse_google_response(OK_BODY).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].label, "Taipei 101, No. 7, Section 5, Xinyi Rd, Taiwan");
        assert_eq!((got[0].latitude, got[0].longitude), (25.0339639, 121.5644722));
    }

    #[test]
    fn caps_candidates_at_five() {
        let item = r#"{"formatted_address":"x","geometry":{"location":{"lat":1.0,"lng":2.0}}}"#;
        let body = format!(r#"{{"status":"OK","results":[{}]}}"#, vec![item; 9].join(","));
        assert_eq!(parse_google_response(&body).unwrap().len(), 5);
    }

    #[test]
    fn zero_results_is_an_empty_list_not_an_error() {
        assert_eq!(parse_google_response(r#"{"status":"ZERO_RESULTS","results":[]}"#).unwrap(), vec![]);
    }

    #[test]
    fn denied_requests_surface_googles_own_message() {
        let body = r#"{"status":"REQUEST_DENIED","error_message":"The provided API key is invalid.","results":[]}"#;
        match parse_google_response(body) {
            Err(GeocodeError::Rejected(msg)) => assert_eq!(msg, "The provided API key is invalid."),
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn a_rejection_without_a_message_falls_back_to_the_status_code() {
        match parse_google_response(r#"{"status":"INVALID_REQUEST"}"#) {
            Err(GeocodeError::Rejected(msg)) => assert_eq!(msg, "INVALID_REQUEST"),
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn quota_statuses_map_to_the_quota_error() {
        for status in ["OVER_QUERY_LIMIT", "OVER_DAILY_LIMIT"] {
            let body = format!(r#"{{"status":"{status}"}}"#);
            assert!(matches!(parse_google_response(&body), Err(GeocodeError::OverQuota)));
        }
    }

    #[test]
    fn garbage_bodies_are_a_clean_error() {
        assert!(matches!(parse_google_response("<html>502</html>"), Err(GeocodeError::BadResponse)));
    }

    #[tokio::test]
    async fn search_rejects_blank_queries_and_missing_keys_before_any_network_call() {
        assert!(matches!(search(Provider::Osm, "   ", None).await, Err(GeocodeError::EmptyQuery)));
        assert!(matches!(search(Provider::Google, "   ", Some("k")).await, Err(GeocodeError::EmptyQuery)));
        assert!(matches!(search(Provider::Google, "Taipei", None).await, Err(GeocodeError::NoApiKey)));
        assert!(matches!(search(Provider::Google, "Taipei", Some("")).await, Err(GeocodeError::NoApiKey)));
    }

    #[test]
    fn coordinate_validation_rejects_out_of_range_and_nan() {
        assert!(validate_coordinates(25.0, 121.5).is_ok());
        assert!(validate_coordinates(-90.0, 180.0).is_ok());
        assert!(validate_coordinates(90.5, 0.0).is_err());
        assert!(validate_coordinates(0.0, -181.0).is_err());
        assert!(validate_coordinates(f64::NAN, 0.0).is_err());
    }

    #[test]
    fn parses_nominatim_results_with_string_coordinates() {
        let body = r#"[
            {"place_id": 1, "lat": "25.0339639", "lon": "121.5644722", "display_name": "Taipei 101, Xinyi District, Taipei, Taiwan"},
            {"place_id": 2, "lat": "25.04", "lon": "121.56", "display_name": "Taipei, Taiwan"}
        ]"#;
        let got = parse_osm_response(body).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].label, "Taipei 101, Xinyi District, Taipei, Taiwan");
        assert_eq!((got[0].latitude, got[0].longitude), (25.0339639, 121.5644722));
    }

    #[test]
    fn nominatim_empty_array_is_no_results_and_unparseable_items_are_skipped() {
        assert_eq!(parse_osm_response("[]").unwrap(), vec![]);
        let body = r#"[
            {"lat": "not-a-number", "lon": "1", "display_name": "bad"},
            {"lat": "1.5", "lon": "2.5", "display_name": "good"}
        ]"#;
        let got = parse_osm_response(body).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].label, "good");
    }

    #[test]
    fn nominatim_error_objects_and_html_are_a_clean_error() {
        assert!(matches!(parse_osm_response(r#"{"error":"rate limited"}"#), Err(GeocodeError::BadResponse)));
        assert!(matches!(parse_osm_response("<html>"), Err(GeocodeError::BadResponse)));
    }

    #[test]
    fn nominatim_results_are_capped_at_five() {
        let item = r#"{"lat":"1","lon":"2","display_name":"x"}"#;
        let body = format!("[{}]", vec![item; 9].join(","));
        assert_eq!(parse_osm_response(&body).unwrap().len(), 5);
    }

    #[test]
    fn rate_limit_spaces_consecutive_requests_by_the_minimum_interval() {
        let t0 = Instant::now();
        let gap = Duration::from_millis(1100);
        assert_eq!(wait_for_slot(None, t0, gap), Duration::ZERO);
        // Previous request started 300ms ago: wait the remaining 800ms.
        assert_eq!(wait_for_slot(Some(t0), t0 + Duration::from_millis(300), gap), Duration::from_millis(800));
        // Long enough ago: no wait.
        assert_eq!(wait_for_slot(Some(t0), t0 + Duration::from_secs(5), gap), Duration::ZERO);
    }

    #[test]
    fn provider_settings_round_trip_and_unknown_values_fall_back_to_osm() {
        assert_eq!(Provider::from_setting(Some(Provider::Google.as_setting())), Provider::Google);
        assert_eq!(Provider::from_setting(Some(Provider::Osm.as_setting())), Provider::Osm);
        assert_eq!(Provider::from_setting(None), Provider::Osm);
        assert_eq!(Provider::from_setting(Some("bing")), Provider::Osm);
    }
}
