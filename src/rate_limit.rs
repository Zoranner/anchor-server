use std::{
    collections::{HashMap, VecDeque},
    net::{IpAddr, SocketAddr},
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::http::HeaderMap;

const MAX_TRACKED_KEYS: usize = 10_000;
const STALE_KEY_AGE: Duration = Duration::from_secs(60 * 60);

#[derive(Default)]
pub struct RequestLimiter {
    entries: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl RequestLimiter {
    pub fn check(&self, key: String, limit: usize, window: Duration) -> bool {
        self.check_at(key, limit, window, Instant::now())
    }

    fn check_at(&self, key: String, limit: usize, window: Duration, now: Instant) -> bool {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if entries.len() > MAX_TRACKED_KEYS {
            entries.retain(|_, queue| {
                queue
                    .back()
                    .is_some_and(|last| now.saturating_duration_since(*last) < STALE_KEY_AGE)
            });
        }
        if entries.len() >= MAX_TRACKED_KEYS && !entries.contains_key(&key) {
            return false;
        }

        let queue = entries.entry(key).or_default();
        while queue
            .front()
            .is_some_and(|value| now.saturating_duration_since(*value) >= window)
        {
            queue.pop_front();
        }
        if queue.len() >= limit {
            return false;
        }
        queue.push_back(now);
        true
    }
}

pub fn client_ip(headers: &HeaderMap, peer: SocketAddr, trust_proxy_headers: bool) -> IpAddr {
    if trust_proxy_headers {
        if let Some(forwarded) = headers
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .and_then(|value| value.trim().parse().ok())
        {
            return forwarded;
        }
    }
    peer.ip()
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn limiter_uses_a_sliding_window() {
        let limiter = RequestLimiter::default();
        let start = Instant::now();
        let window = Duration::from_secs(600);

        for _ in 0..5 {
            assert!(limiter.check_at("installation:one".into(), 5, window, start));
        }
        assert!(!limiter.check_at("installation:one".into(), 5, window, start));
        assert!(limiter.check_at(
            "installation:one".into(),
            5,
            window,
            start + Duration::from_secs(601),
        ));
    }

    #[test]
    fn proxy_headers_are_only_used_when_enabled() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.8, 10.0.0.2"),
        );
        let peer: SocketAddr = "10.0.0.2:443".parse().unwrap();

        assert_eq!(
            client_ip(&headers, peer, false),
            "10.0.0.2".parse::<IpAddr>().unwrap()
        );
        assert_eq!(
            client_ip(&headers, peer, true),
            "203.0.113.8".parse::<IpAddr>().unwrap()
        );
    }
}
