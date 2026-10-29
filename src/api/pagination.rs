use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::error::ApiError;

#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

pub fn limit(value: Option<u64>) -> Result<u64, ApiError> {
    let limit = value.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("limit must be between 1 and 100"));
    }
    Ok(limit)
}

pub fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> Option<String> {
    created_at
        .timestamp_nanos_opt()
        .map(|value| format!("{value}:{id}"))
}

pub fn parse_cursor(value: &str) -> Result<(DateTime<Utc>, Uuid), ApiError> {
    let (nanos, id) = value
        .split_once(':')
        .ok_or_else(|| ApiError::bad_request("invalid cursor"))?;
    let nanos = nanos
        .parse::<i64>()
        .map_err(|_| ApiError::bad_request("invalid cursor"))?;
    let seconds = nanos.div_euclid(1_000_000_000);
    let subsec_nanos = nanos.rem_euclid(1_000_000_000) as u32;
    let created_at = DateTime::from_timestamp(seconds, subsec_nanos)
        .ok_or_else(|| ApiError::bad_request("invalid cursor"))?;
    let id = Uuid::parse_str(id).map_err(|_| ApiError::bad_request("invalid cursor"))?;
    Ok((created_at, id))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn cursor_round_trips_timestamp_and_id() {
        let created_at = Utc.with_ymd_and_hms(2026, 10, 29, 8, 0, 0).unwrap();
        let id = Uuid::new_v4();
        let cursor = encode_cursor(created_at, id).unwrap();

        assert_eq!(parse_cursor(&cursor).unwrap(), (created_at, id));
        assert!(parse_cursor("invalid").is_err());
    }
}
