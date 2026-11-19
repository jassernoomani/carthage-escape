mod common;

use serde_json::{Value, json};
use sqlx::PgPool;

async fn code_of(res: reqwest::Response) -> (u16, String) {
    let status = res.status().as_u16();
    let body: Value = res.json().await.unwrap();
    (status, body["code"].as_str().unwrap_or_default().to_owned())
}

#[sqlx::test]
async fn register_login_me_logout(pool: PgPool) {
    let app = common::spawn(pool).await;
    let client = app.client();

    let res = app.register(&client, "Teacher@School.tn").await;
    assert_eq!(res.status(), 201);
    let cookie = res.headers()["set-cookie"].to_str().unwrap().to_owned();
    assert!(
        cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"),
        "{cookie}"
    );

    let me: Value = client
        .get(app.url("/api/me"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me["host"]["email"], "teacher@school.tn");
    assert_eq!(me["host"]["display_name"], "Ms Teacher");
    assert!(me["host"].get("password_hash").is_none());

    let res = client
        .post(app.url("/api/auth/logout"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
    assert_eq!(
        client
            .get(app.url("/api/me"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );

    let login = |password: &'static str| {
        client
            .post(app.url("/api/auth/login"))
            .json(&json!({"email": "TEACHER@school.tn", "password": password}))
            .send()
    };
    assert_eq!(
        code_of(login("wrong-password").await.unwrap()).await,
        (401, "invalid_credentials".into())
    );
    assert_eq!(login("password123").await.unwrap().status(), 200);
    assert_eq!(
        client
            .get(app.url("/api/me"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
}

#[sqlx::test]
async fn logout_revokes_the_session_server_side(pool: PgPool) {
    let app = common::spawn(pool).await;
    let client = app.client();
    let res = app.register(&client, "a@b.co").await;
    let cookie = res.headers()["set-cookie"].to_str().unwrap();
    let token_pair = cookie.split(';').next().unwrap().to_owned();

    client
        .post(app.url("/api/auth/logout"))
        .send()
        .await
        .unwrap();

    // Replaying the old cookie value must not work anymore.
    let res = reqwest::Client::new()
        .get(app.url("/api/me"))
        .header("cookie", token_pair)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[sqlx::test]
async fn register_validates_input(pool: PgPool) {
    let app = common::spawn(pool).await;
    assert_eq!(app.register(&app.client(), "dup@x.com").await.status(), 201);
    assert_eq!(
        code_of(app.register(&app.client(), "DUP@x.com").await).await,
        (409, "email_taken".into())
    );

    let res = app
        .client()
        .post(app.url("/api/auth/register"))
        .json(&json!({"email": "new@x.com", "password": "short", "display_name": "X"}))
        .send()
        .await
        .unwrap();
    assert_eq!(code_of(res).await, (400, "validation".into()));
}

#[sqlx::test]
async fn unknown_email_login_fails_like_wrong_password(pool: PgPool) {
    let app = common::spawn(pool).await;
    let res = app
        .client()
        .post(app.url("/api/auth/login"))
        .json(&json!({"email": "nobody@x.com", "password": "password123"}))
        .send()
        .await
        .unwrap();
    assert_eq!(code_of(res).await, (401, "invalid_credentials".into()));
}

#[sqlx::test]
async fn host_endpoints_require_login(pool: PgPool) {
    let app = common::spawn(pool).await;
    let anon = app.client();
    for path in ["/api/me", "/api/rooms", "/api/sessions"] {
        let res = anon.get(app.url(path)).send().await.unwrap();
        assert_eq!(code_of(res).await, (401, "unauthorized".into()), "{path}");
    }
    let res = anon
        .post(app.url("/api/sessions"))
        .json(&json!({"template_id": "carthage-01"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[sqlx::test]
async fn host_creates_lists_and_views_sessions(pool: PgPool) {
    let app = common::spawn(pool).await;
    let (client, id, code, host_token) = app.host_with_session("host@x.com").await;
    assert_eq!(code.len(), 6);
    assert!(!host_token.is_empty());

    let rooms: Value = client
        .get(app.url("/api/rooms"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(rooms[0]["id"], "carthage-01");
    assert_eq!(rooms[0]["puzzle_count"], 5);

    let list: Value = client
        .get(app.url("/api/sessions"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list[0]["id"], id.as_str());
    assert_eq!(list[0]["status"], "lobby");
    assert_eq!(list[0]["player_count"], 0);

    let detail: Value = client
        .get(app.url(&format!("/api/sessions/{id}")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail["code"], code.as_str());
    assert!(detail["host_token"].is_string());

    // Another host can't see it.
    let other = app.client();
    app.register(&other, "other@x.com").await;
    let res = other
        .get(app.url(&format!("/api/sessions/{id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
    let list: Value = other
        .get(app.url("/api/sessions"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list, json!([]));
}

#[sqlx::test]
async fn unknown_template_is_404(pool: PgPool) {
    let app = common::spawn(pool).await;
    let client = app.client();
    app.register(&client, "h@x.com").await;
    let res = client
        .post(app.url("/api/sessions"))
        .json(&json!({"template_id": "nope"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[sqlx::test]
async fn players_join_with_code_and_nickname(pool: PgPool) {
    let app = common::spawn(pool).await;
    let (_, _, code, _) = app.host_with_session("host@x.com").await;

    // Codes are case-insensitive and whitespace-tolerant.
    let messy = format!(" {} ", code.to_lowercase());
    let res = app.join(&messy, "  Yasmine ").await;
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["nickname"], "Yasmine");
    assert_eq!(body["code"], code.as_str());
    assert!(body["player_token"].is_string());

    assert_eq!(
        code_of(app.join(&code, "yasmine").await).await,
        (409, "nickname_taken".into())
    );
    assert_eq!(
        code_of(app.join(&code, "sh1t").await).await,
        (400, "nickname_invalid".into())
    );
    assert_eq!(
        code_of(app.join(&code, "seventeen-chars-x").await).await,
        (400, "nickname_invalid".into())
    );
    assert_eq!(app.join(&code, "Sami").await.status(), 200);
    assert_eq!(
        code_of(app.join(&code, "Third").await).await,
        (409, "room_full".into())
    );
    assert_eq!(
        code_of(app.join("ZZZZZZ", "Sami").await).await,
        (404, "room_not_found".into())
    );

    let players =
        sqlx::query!("SELECT nickname, role_index FROM session_players ORDER BY role_index")
            .fetch_all(&app.pool)
            .await
            .unwrap();
    let players: Vec<_> = players
        .into_iter()
        .map(|p| (p.nickname, p.role_index))
        .collect();
    assert_eq!(
        players,
        vec![("Yasmine".to_owned(), 0), ("Sami".to_owned(), 1)]
    );
}

#[sqlx::test]
async fn join_is_rate_limited(pool: PgPool) {
    let app = common::spawn(pool).await;
    for _ in 0..10 {
        assert_eq!(app.join("ZZZZZZ", "Sami").await.status(), 404);
    }
    assert_eq!(
        code_of(app.join("ZZZZZZ", "Sami").await).await,
        (429, "too_many_requests".into())
    );
}

#[sqlx::test]
async fn deleting_a_session_closes_the_room(pool: PgPool) {
    let app = common::spawn(pool).await;
    let (client, id, code, _) = app.host_with_session("host@x.com").await;

    let res = client
        .delete(app.url(&format!("/api/sessions/{id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
    assert_eq!(
        code_of(app.join(&code, "Sami").await).await.1,
        "room_not_found"
    );
    let res = client
        .get(app.url(&format!("/api/sessions/{id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}
