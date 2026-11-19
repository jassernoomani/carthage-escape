//! Test helpers: spawn the real app on a random port, plus small HTTP/WebSocket clients.
#![allow(dead_code)] // each test binary uses a different subset

use std::{net::SocketAddr, path::Path, time::Duration};

use carthage_server::{AppState, content};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::net::TcpListener;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};

pub const SECRET: &[u8] = b"test-secret-test-secret-test-secret!";

pub struct TestApp {
    pub base: String,
    pub addr: SocketAddr,
    pub pool: PgPool,
}

pub async fn spawn(pool: PgPool) -> TestApp {
    let content_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../content");
    content::sync_rooms(&pool, &content_dir).await.unwrap();

    let app = carthage_server::app(AppState::new(pool.clone(), SECRET, false), None);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    TestApp {
        base: format!("http://{addr}"),
        addr,
        pool,
    }
}

impl TestApp {
    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// A client with its own cookie jar (one per simulated browser).
    pub fn client(&self) -> reqwest::Client {
        reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap()
    }

    pub async fn register(&self, client: &reqwest::Client, email: &str) -> reqwest::Response {
        client
            .post(self.url("/api/auth/register"))
            .json(&json!({"email": email, "password": "password123", "display_name": "Ms Teacher"}))
            .send()
            .await
            .unwrap()
    }

    /// Registers a host and creates a session. Returns (client, session id, code, host token).
    pub async fn host_with_session(
        &self,
        email: &str,
    ) -> (reqwest::Client, String, String, String) {
        let client = self.client();
        assert_eq!(self.register(&client, email).await.status(), 201);
        let res = client
            .post(self.url("/api/sessions"))
            .json(&json!({"template_id": "carthage-01"}))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let body: Value = res.json().await.unwrap();
        (
            client,
            body["id"].as_str().unwrap().to_owned(),
            body["code"].as_str().unwrap().to_owned(),
            body["host_token"].as_str().unwrap().to_owned(),
        )
    }

    pub async fn join(&self, code: &str, nickname: &str) -> reqwest::Response {
        self.client()
            .post(self.url("/api/join"))
            .json(&json!({"code": code, "nickname": nickname}))
            .send()
            .await
            .unwrap()
    }

    /// Joins and returns the player token.
    pub async fn join_ok(&self, code: &str, nickname: &str) -> String {
        let res = self.join(code, nickname).await;
        assert_eq!(res.status(), 200, "join as {nickname}");
        let body: Value = res.json().await.unwrap();
        body["player_token"].as_str().unwrap().to_owned()
    }

    pub async fn ws(&self, token: &str) -> WsClient {
        let url = format!("ws://{}/ws?token={token}", self.addr);
        let (stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        WsClient { stream }
    }
}

pub struct WsClient {
    stream: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
}

pub enum WsEvent {
    Msg(Value),
    Closed(Option<u16>),
}

impl WsClient {
    pub async fn send(&mut self, msg: Value) {
        self.stream
            .send(Message::Text(msg.to_string().into()))
            .await
            .unwrap();
    }

    pub async fn send_raw(&mut self, text: &str) {
        self.stream
            .send(Message::Text(text.to_owned().into()))
            .await
            .unwrap();
    }

    pub async fn next_event(&mut self) -> WsEvent {
        loop {
            let next = tokio::time::timeout(Duration::from_secs(5), self.stream.next())
                .await
                .expect("timed out waiting for a WebSocket message");
            match next {
                Some(Ok(Message::Text(text))) => {
                    return WsEvent::Msg(serde_json::from_str(&text).unwrap());
                }
                Some(Ok(Message::Close(frame))) => {
                    return WsEvent::Closed(frame.map(|f| f.code.into()));
                }
                Some(Ok(_)) => continue,
                Some(Err(_)) | None => return WsEvent::Closed(None),
            }
        }
    }

    pub async fn recv(&mut self) -> Value {
        match self.next_event().await {
            WsEvent::Msg(v) => v,
            WsEvent::Closed(code) => panic!("socket closed ({code:?}) while waiting for a message"),
        }
    }

    /// Reads messages until one of the given type arrives, and returns it.
    pub async fn recv_type(&mut self, ty: &str) -> Value {
        loop {
            let msg = self.recv().await;
            if msg["type"] == ty {
                return msg;
            }
        }
    }

    /// Expects the socket to close; returns the close code.
    pub async fn expect_closed(&mut self) -> Option<u16> {
        loop {
            if let WsEvent::Closed(code) = self.next_event().await {
                return code;
            }
        }
    }
}
