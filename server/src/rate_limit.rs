//! Small in-memory fixed-window rate limiter, keyed by (bucket, client IP).
//! Good enough for a single server instance; would move to Redis with several instances.

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::request::Parts,
};

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    Join,
    Login,
    Register,
}

impl Bucket {
    fn limit(self) -> (u32, Duration) {
        match self {
            // A whole classroom shares one school IP (NAT), so this must fit ~30 kids joining at
            // once. Guessing codes is hopeless anyway: 31^6 ≈ 887M possible codes.
            Bucket::Join => (60, Duration::from_secs(60)),
            Bucket::Login => (10, Duration::from_secs(60)),
            Bucket::Register => (5, Duration::from_secs(600)),
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
    windows: Arc<Mutex<HashMap<(Bucket, IpAddr), Window>>>,
}

impl RateLimiter {
    pub fn check(&self, bucket: Bucket, ip: IpAddr) -> Result<(), AppError> {
        if self.allow(bucket, ip, Instant::now()) {
            Ok(())
        } else {
            Err(AppError::TooManyRequests)
        }
    }

    fn allow(&self, bucket: Bucket, ip: IpAddr, now: Instant) -> bool {
        let (limit, period) = bucket.limit();
        // A poisoned lock only means another thread panicked mid-update; the map is still usable.
        let mut windows = self.windows.lock().unwrap_or_else(|e| e.into_inner());

        if windows.len() > 10_000 {
            windows.retain(|(b, _), w| now.duration_since(w.started) < b.limit().1);
        }

        let window = windows.entry((bucket, ip)).or_insert(Window {
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

/// The client's IP address. Falls back to 0.0.0.0 when connection info is unavailable (tests).
/// TODO(M6): when running behind Caddy, read the real client IP from X-Forwarded-For.
pub struct ClientIp(pub IpAddr);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip())
            .unwrap_or(IpAddr::from([0, 0, 0, 0]));
        Ok(ClientIp(ip))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_per_bucket_and_ip_and_resets() {
        let limiter = RateLimiter::default();
        let ip: IpAddr = [10, 0, 0, 1].into();
        let other: IpAddr = [10, 0, 0, 2].into();
        let t0 = Instant::now();

        for _ in 0..60 {
            assert!(limiter.allow(Bucket::Join, ip, t0));
        }
        assert!(
            !limiter.allow(Bucket::Join, ip, t0),
            "61st join in a minute is blocked"
        );
        assert!(
            limiter.allow(Bucket::Join, other, t0),
            "other IPs are unaffected"
        );
        assert!(
            limiter.allow(Bucket::Login, ip, t0),
            "other buckets are unaffected"
        );
        assert!(
            limiter.allow(Bucket::Join, ip, t0 + Duration::from_secs(61)),
            "window resets"
        );
    }
}
