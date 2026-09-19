//! Forward geocoding via Google's Geocoding API (RFC-0007 §3.2).
//!
//! The only thing sent off-device is the user's typed search string plus
//! their own API key, and only when they press Search. Results are returned
//! to the caller and never persisted here (Google's terms restrict storing
//! geocoding results); the caller persists only coordinates the user picks.
//!
//! Response parsing is a pure function so it is tested against recorded
//! JSON bodies -- CI never touches the network.

use serde::{Deserialize, Serialize};
use std::time::Duration;

const ENDPOINT: &str = "https://maps.googleapis.com/maps/api/geocode/json";
const MAX_CANDIDATES: usize = 5;

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
    #[error("No Google Maps API key is set. Add one in Settings → Map.")]
    NoApiKey,
    /// Transport failure. The URL is stripped from the underlying error
    /// before it gets here: it contains the API key.
    #[error("Couldn't reach Google Maps: {0}")]
    Network(String),
    #[error("Google Maps returned an unreadable response.")]
    BadResponse,
    #[error("Google Maps rejected the request: {0}")]
    Rejected(String),
    #[error("Google Maps usage limit reached. Try again later or check your quota.")]
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
pub fn parse_response(body: &str) -> Result<Vec<GeocodeCandidate>, GeocodeError> {
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

pub async fn search(query: &str, api_key: Option<&str>) -> Result<Vec<GeocodeCandidate>, GeocodeError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(GeocodeError::EmptyQuery);
    }
    let api_key = api_key.filter(|k| !k.is_empty()).ok_or(GeocodeError::NoApiKey)?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| GeocodeError::Network(e.without_url().to_string()))?;
    let body = client
        .get(ENDPOINT)
        .query(&[("address", query), ("key", api_key)])
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| GeocodeError::Network(e.without_url().to_string()))?
        .text()
        .await
        .map_err(|e| GeocodeError::Network(e.without_url().to_string()))?;
    parse_response(&body)
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
        let got = parse_response(OK_BODY).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].label, "Taipei 101, No. 7, Section 5, Xinyi Rd, Taiwan");
        assert_eq!((got[0].latitude, got[0].longitude), (25.0339639, 121.5644722));
    }

    #[test]
    fn caps_candidates_at_five() {
        let item = r#"{"formatted_address":"x","geometry":{"location":{"lat":1.0,"lng":2.0}}}"#;
        let body = format!(r#"{{"status":"OK","results":[{}]}}"#, vec![item; 9].join(","));
        assert_eq!(parse_response(&body).unwrap().len(), 5);
    }

    #[test]
    fn zero_results_is_an_empty_list_not_an_error() {
        assert_eq!(parse_response(r#"{"status":"ZERO_RESULTS","results":[]}"#).unwrap(), vec![]);
    }

    #[test]
    fn denied_requests_surface_googles_own_message() {
        let body = r#"{"status":"REQUEST_DENIED","error_message":"The provided API key is invalid.","results":[]}"#;
        match parse_response(body) {
            Err(GeocodeError::Rejected(msg)) => assert_eq!(msg, "The provided API key is invalid."),
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn a_rejection_without_a_message_falls_back_to_the_status_code() {
        match parse_response(r#"{"status":"INVALID_REQUEST"}"#) {
            Err(GeocodeError::Rejected(msg)) => assert_eq!(msg, "INVALID_REQUEST"),
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn quota_statuses_map_to_the_quota_error() {
        for status in ["OVER_QUERY_LIMIT", "OVER_DAILY_LIMIT"] {
            let body = format!(r#"{{"status":"{status}"}}"#);
            assert!(matches!(parse_response(&body), Err(GeocodeError::OverQuota)));
        }
    }

    #[test]
    fn garbage_bodies_are_a_clean_error() {
        assert!(matches!(parse_response("<html>502</html>"), Err(GeocodeError::BadResponse)));
    }

    #[tokio::test]
    async fn search_rejects_blank_queries_and_missing_keys_before_any_network_call() {
        assert!(matches!(search("   ", Some("k")).await, Err(GeocodeError::EmptyQuery)));
        assert!(matches!(search("Taipei", None).await, Err(GeocodeError::NoApiKey)));
        assert!(matches!(search("Taipei", Some("")).await, Err(GeocodeError::NoApiKey)));
    }

    #[test]
    fn coordinate_validation_rejects_out_of_range_and_nan() {
        assert!(validate_coordinates(25.0, 121.5).is_ok());
        assert!(validate_coordinates(-90.0, 180.0).is_ok());
        assert!(validate_coordinates(90.5, 0.0).is_err());
        assert!(validate_coordinates(0.0, -181.0).is_err());
        assert!(validate_coordinates(f64::NAN, 0.0).is_err());
    }
}
