pub mod api;
pub mod auth;
pub mod config;
pub mod content;
pub mod db;
pub mod error;
pub mod game;
pub mod nickname;
pub mod rate_limit;
pub mod ws;

use std::{path::Path, sync::Arc};

use axum::{
    Router,
    http::{HeaderName, HeaderValue, Request, header},
    routing::get,
};
use sqlx::PgPool;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

use crate::{auth::TokenKeys, game::RoomManager, rate_limit::RateLimiter};

/// Shared state handed to every request handler.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub rooms: RoomManager,
    pub tokens: Arc<TokenKeys>,
    pub limiter: RateLimiter,
    pub cookie_secure: bool,
    pub trust_proxy: bool,
}

impl AppState {
    pub fn new(db: PgPool, jwt_secret: &[u8], cookie_secure: bool) -> Self {
        Self {
            db,
            rooms: RoomManager::default(),
            tokens: Arc::new(TokenKeys::new(jwt_secret)),
            limiter: RateLimiter::default(),
            cookie_secure,
            trust_proxy: false,
        }
    }

    pub fn with_trust_proxy(mut self, trust_proxy: bool) -> Self {
        self.trust_proxy = trust_proxy;
        self
    }
}

/// Content Security Policy for the app: only our own scripts, styles and sockets. No inline
/// scripts exist (Vite emits files), so none are allowed. `ws:`/`wss:` are listed because older
/// Safari versions don't treat same-host WebSockets as `'self'`.
const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
                   font-src 'self'; connect-src 'self' ws: wss:; object-src 'none'; \
                   base-uri 'self'; form-action 'self'; frame-ancestors 'none'";

/// Builds the full HTTP application.
///
/// When `static_dir` is set (production), the built frontend is served from it: hashed files
/// under `/assets` are cached forever, everything else falls back to `index.html` (never cached)
/// so client-side routing works.
pub fn app(state: AppState, static_dir: Option<&Path>) -> Router {
    let mut router = Router::new()
        .nest("/api", api::router())
        .route("/ws", get(ws::ws_handler))
        .with_state(state);

    if let Some(dir) = static_dir {
        let assets = ServiceBuilder::new()
            .layer(SetResponseHeaderLayer::overriding(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000, immutable"),
            ))
            .layer(CompressionLayer::new())
            .service(ServeDir::new(dir.join("assets")));
        let spa = ServiceBuilder::new()
            .layer(SetResponseHeaderLayer::overriding(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache"),
            ))
            .layer(CompressionLayer::new())
            .service(ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html"))));
        router = router.nest_service("/assets", assets).fallback_service(spa);
    }

    let security_headers = ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CSP),
        ));

    router.layer(security_headers).layer(
        // Log the path only: WebSocket tokens travel in the query string and must not be logged.
        TraceLayer::new_for_http().make_span_with(|req: &Request<_>| {
            tracing::info_span!("http", method = %req.method(), path = %req.uri().path())
        }),
    )
}
