//! Small in-memory fixed-window rate limiter, keyed by (bucket, key) where the key is a client
//! IP or an account email. Good enough for a single server instance; would move to Redis with
//! several instances.
//!
//! Per-IP limits are deliberately generous: a whole school (kids *and* teachers) usually shares
//! one public IP. Brute force on passwords is stopped by the per-account login limit instead.

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{HeaderMap, request::Parts},
};

use crate::{AppState, error::AppError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    /// Per IP. Must fit ~30 kids joining at once from one school IP; guessing codes is hopeless
    /// anyway (31^6 ≈ 887M possible codes).
    Join,
    /// Per IP: a staff room of teachers logging in together.
    LoginIp,
    /// Per account (email): stops password guessing regardless of IP.
    LoginAccount,
    /// Per IP: e.g. a teacher-training session where everyone signs up together.
    Register,
}

impl Bucket {
    fn limit(self) -> (u32, Duration) {
        match self {
            Bucket::Join => (60, Duration::from_secs(60)),
            Bucket::LoginIp => (30, Duration::from_secs(60)),
            Bucket::LoginAccount => (10, Duration::from_secs(600)),
            Bucket::Register => (30, Duration::from_secs(600)),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Window {
    started: Instant,
    count: u32,
}

#[derive(Clone, Default)]
pub struct RateLimiter {
    windows: Arc<Mutex<HashMap<(Bucket, String), Window>>>,
}

impl RateLimiter {
    /// Counts one attempt for `key` (an IP address or an email) and fails once over the limit.
    pub fn check(&self, bucket: Bucket, key: impl ToString) -> Result<(), AppError> {
        if self.allow(bucket, key.to_string(), Instant::now()) {
            Ok(())
        } else {
            Err(AppError::TooManyRequests)
        }
    }

    fn allow(&self, bucket: Bucket, key: String, now: Instant) -> bool {
        let (limit, period) = bucket.limit();
        // A poisoned lock only means another thread panicked mid-update; the map is still usable.
        let mut windows = self.windows.lock().unwrap_or_else(|e| e.into_inner());

        if windows.len() > 10_000 {
            windows.retain(|(b, _), w| now.duration_since(w.started) < b.limit().1);
        }

        let window = windows.entry((bucket, key)).or_insert(Window {
            started: now,
            count: 0,
        });
        if now.duration_since(window.started) >= period {
            *window = Window {
                started: now,
                count: 0,
            };
        }
        if window.count >= limit {
            return false;
        }
        window.count += 1;
        true
    }
}

/// The client's IP address.
///
/// With `TRUST_PROXY=true` (behind exactly one reverse proxy such as Caddy), it is the last
/// `X-Forwarded-For` entry: the address the proxy itself saw. Earlier entries are ignored
/// because the client can forge them. Otherwise it is the TCP peer address, falling back to
/// 0.0.0.0 when connection info is unavailable (tests).
pub struct ClientIp(pub IpAddr);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let forwarded = state
            .trust_proxy
            .then(|| last_forwarded_for(&parts.headers))
            .flatten();
        let ip = forwarded
            .or_else(|| {
                parts
                    .extensions
                    .get::<ConnectInfo<SocketAddr>>()
                    .map(|ConnectInfo(addr)| addr.ip())
            })
            .unwrap_or(IpAddr::from([0, 0, 0, 0]));
        Ok(ClientIp(ip))
    }
}

fn last_forwarded_for(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .next_back()
        .and_then(|ip| ip.trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwarded_for_uses_the_entry_added_by_our_proxy() {
        let mut headers = HeaderMap::new();
        assert_eq!(last_forwarded_for(&headers), None);
        // The client forged "6.6.6.6"; our proxy appended the real peer, 203.0.113.7.
        headers.insert("x-forwarded-for", "6.6.6.6, 203.0.113.7".parse().unwrap());
        assert_eq!(last_forwarded_for(&headers), Some([203, 0, 113, 7].into()));
        headers.insert("x-forwarded-for", "garbage".parse().unwrap());
        assert_eq!(last_forwarded_for(&headers), None);
    }

    #[test]
    fn limits_per_bucket_and_ip_and_resets() {
        let limiter = RateLimiter::default();
        let ip: IpAddr = [10, 0, 0, 1].into();
        let other: IpAddr = [10, 0, 0, 2].into();
        let t0 = Instant::now();

        let (ip, other) = (ip.to_string(), other.to_string());
        for _ in 0..60 {
            assert!(limiter.allow(Bucket::Join, ip.clone(), t0));
        }
        assert!(
            !limiter.allow(Bucket::Join, ip.clone(), t0),
            "61st join in a minute is blocked"
        );
        assert!(
            limiter.allow(Bucket::Join, other, t0),
            "other IPs are unaffected"
        );
        assert!(
            limiter.allow(Bucket::LoginIp, ip.clone(), t0),
            "other buckets are unaffected"
        );
        assert!(
            limiter.allow(Bucket::Join, ip, t0 + Duration::from_secs(61)),
            "window resets"
        );
    }
}
