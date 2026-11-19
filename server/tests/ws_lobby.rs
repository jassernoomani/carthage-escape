//! End-to-end over real sockets: the M1 "done" scenario (two players meet in the lobby and
//! the host starts the game), plus the WebSocket error paths.

mod common;

use serde_json::json;
use sqlx::PgPool;

#[sqlx::test]
async fn two_players_meet_in_lobby_and_host_starts(pool: PgPool) {
    let app = common::spawn(pool).await;
    let (_, session_id, code, host_token) = app.host_with_session("host@x.com").await;

    let mut host = app.ws(&host_token).await;
    let welcome = host.recv_type("welcome").await;
    assert_eq!(welcome["data"]["you"]["kind"], "host");
    assert_eq!(welcome["data"]["room"]["code"], code.as_str());
    assert_eq!(welcome["data"]["room"]["players_needed"], 2);
    assert_eq!(welcome["data"]["status"], "lobby");

    // Player A joins over REST: the host sees them appear (not connected yet).
    let token_a = app.join_ok(&code, "Ali").await;
    let update = host.recv_type("lobby_update").await;
    assert_eq!(
        update["data"]["players"],
        json!([{"nickname": "Ali", "role_index": 0, "connected": false, "ready": false}])
    );

    let mut a = app.ws(&token_a).await;
    let welcome = a.recv_type("welcome").await;
    assert_eq!(welcome["data"]["you"]["kind"], "player");
    assert_eq!(welcome["data"]["you"]["nickname"], "Ali");
    assert!(welcome["seq"].as_u64().unwrap() >= 1);
    assert_eq!(
        host.recv_type("lobby_update").await["data"]["players"][0]["connected"],
        true
    );

    let token_b = app.join_ok(&code, "Bea").await;
    let mut b = app.ws(&token_b).await;
    let welcome = b.recv_type("welcome").await;
    // B's snapshot already shows A, connected.
    assert_eq!(welcome["data"]["players"][0]["nickname"], "Ali");
    assert_eq!(welcome["data"]["players"][0]["connected"], true);
    // ...and A sees B arrive.
    let mut seen_b = false;
    while !seen_b {
        let update = a.recv_type("lobby_update").await;
        seen_b = update["data"]["players"][1]["connected"] == true;
    }

    // Starting before everyone is ready is refused.
    host.send(json!({"type": "host_start", "data": {}})).await;
    assert_eq!(
        host.recv_type("error").await["data"]["code"],
        "players_not_ready"
    );

    for p in [&mut a, &mut b] {
        p.send(json!({"type": "ready", "data": {"ready": true}}))
            .await;
    }
    loop {
        let update = host.recv_type("lobby_update").await;
        let players = update["data"]["players"].as_array().unwrap().clone();
        if players.iter().all(|p| p["ready"] == true) {
            break;
        }
    }

    host.send(json!({"type": "host_start", "data": {}})).await;
    let mut ends = Vec::new();
    for socket in [&mut host, &mut a, &mut b] {
        ends.push(
            socket.recv_type("game_started").await["data"]["ends_at"]
                .as_i64()
                .unwrap(),
        );
    }
    assert!(
        ends.windows(2).all(|w| w[0] == w[1]),
        "everyone gets the same end time"
    );

    // Status is persisted (the actor writes it asynchronously, so poll briefly).
    let id: uuid::Uuid = session_id.parse().unwrap();
    let mut status = String::new();
    for _ in 0..50 {
        let row = sqlx::query!(
            r#"SELECT status::text AS "status!", started_at FROM game_sessions WHERE id = $1"#,
            id
        )
        .fetch_one(&app.pool)
        .await
        .unwrap();
        status = row.status;
        if status == "running" {
            assert!(row.started_at.is_some());
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(status, "running");

    // Joining after the start is refused.
    let res = app.join(&code, "Late").await;
    assert_eq!(res.status(), 409);
}

#[sqlx::test]
async fn invalid_token_gets_fatal_error_then_close(pool: PgPool) {
    let app = common::spawn(pool).await;
    let mut ws = app.ws("not-a-real-token").await;
    let msg = ws.recv().await;
    assert_eq!(msg["type"], "error");
    assert_eq!(msg["data"]["code"], "invalid_token");
    assert_eq!(ws.expect_closed().await, Some(4000));
}

#[sqlx::test]
async fn bad_messages_are_reported_without_dropping_the_connection(pool: PgPool) {
    let app = common::spawn(pool).await;
    let (_, _, code, _) = app.host_with_session("host@x.com").await;
    let token = app.join_ok(&code, "Ali").await;
    let mut a = app.ws(&token).await;
    a.recv_type("welcome").await;

    a.send_raw("this is not json").await;
    assert_eq!(a.recv_type("error").await["data"]["code"], "bad_message");

    // Players can't start the game.
    a.send(json!({"type": "host_start", "data": {}})).await;
    assert_eq!(a.recv_type("error").await["data"]["code"], "not_host");

    // Still alive and working.
    a.send(json!({"type": "ready", "data": {"ready": true}}))
        .await;
    assert_eq!(
        a.recv_type("lobby_update").await["data"]["players"][0]["ready"],
        true
    );
}

#[sqlx::test]
async fn second_tab_replaces_the_first(pool: PgPool) {
    let app = common::spawn(pool).await;
    let (_, _, code, _) = app.host_with_session("host@x.com").await;
    let token = app.join_ok(&code, "Ali").await;

    let mut first = app.ws(&token).await;
    first.recv_type("welcome").await;
    let mut second = app.ws(&token).await;
    second.recv_type("welcome").await;

    assert_eq!(first.recv_type("error").await["data"]["code"], "replaced");
    assert_eq!(first.expect_closed().await, Some(4000));
}

#[sqlx::test]
async fn host_deleting_session_kicks_players(pool: PgPool) {
    let app = common::spawn(pool).await;
    let (client, id, code, _) = app.host_with_session("host@x.com").await;
    let token = app.join_ok(&code, "Ali").await;
    let mut a = app.ws(&token).await;
    a.recv_type("welcome").await;

    client
        .delete(app.url(&format!("/api/sessions/{id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(a.recv_type("error").await["data"]["code"], "room_closed");
    assert_eq!(a.expect_closed().await, Some(4000));

    // The token is now useless: the room is gone.
    let mut again = app.ws(&token).await;
    assert_eq!(again.recv().await["data"]["code"], "room_not_found");
}

#[sqlx::test]
async fn missing_token_is_a_fatal_error_not_an_http_error(pool: PgPool) {
    let app = common::spawn(pool).await;
    let mut ws = app.ws("").await;
    assert_eq!(ws.recv().await["data"]["code"], "invalid_token");
    assert_eq!(ws.expect_closed().await, Some(4000));
}

#[sqlx::test]
async fn kid_who_lost_their_tab_rejoins_by_nickname(pool: PgPool) {
    let app = common::spawn(pool).await;
    let (_, _, code, host_token) = app.host_with_session("host@x.com").await;
    let mut host = app.ws(&host_token).await;
    host.recv_type("welcome").await;

    let mut a = app.ws(&app.join_ok(&code, "Ali").await).await;
    a.recv_type("welcome").await;
    let mut b = app.ws(&app.join_ok(&code, "Bea").await).await;
    b.recv_type("welcome").await;

    // While Ali is connected, the name is taken.
    assert_eq!(app.join(&code, "ALI").await.status(), 409);

    for p in [&mut a, &mut b] {
        p.send(json!({"type": "ready", "data": {"ready": true}}))
            .await;
    }
    loop {
        let u = host.recv_type("lobby_update").await;
        if u["data"]["players"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["ready"] == true)
        {
            break;
        }
    }
    host.send(json!({"type": "host_start", "data": {}})).await;
    b.recv_type("game_started").await;

    // Ali's tab closes: Bea is told, and the game keeps going.
    drop(a);
    let note = b.recv_type("player_connection").await;
    assert_eq!(note["data"], json!({"nickname": "Ali", "connected": false}));

    // Ali opens a fresh tab (no token) and types the same name: same seat, game in progress.
    let token = app.join_ok(&code, "ali").await;
    let mut a2 = app.ws(&token).await;
    let welcome = a2.recv_type("welcome").await;
    assert_eq!(welcome["data"]["status"], "running");
    assert_eq!(welcome["data"]["you"]["role_index"], 0);
    assert_eq!(welcome["data"]["you"]["nickname"], "Ali");
    assert_eq!(
        b.recv_type("player_connection").await["data"]["connected"],
        true
    );

    let count = sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM session_players"#)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(count, 2, "reclaiming must not create a new player");
}

#[sqlx::test]
async fn host_removes_a_player_and_the_seat_frees_up(pool: PgPool) {
    let app = common::spawn(pool).await;
    let (_, _, code, host_token) = app.host_with_session("host@x.com").await;
    let mut host = app.ws(&host_token).await;
    host.recv_type("welcome").await;

    let mut a = app.ws(&app.join_ok(&code, "Ali").await).await;
    a.recv_type("welcome").await;
    app.join_ok(&code, "Bea").await;
    assert_eq!(app.join(&code, "Cyd").await.status(), 409, "full");

    host.send(json!({"type": "host_remove_player", "data": {"role_index": 0}}))
        .await;
    assert_eq!(a.recv_type("error").await["data"]["code"], "removed");
    assert_eq!(a.expect_closed().await, Some(4000));

    // Row deleted (actor does it asynchronously) and the seat is free for someone else.
    let mut remaining = -1;
    for _ in 0..50 {
        remaining = sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM session_players"#)
            .fetch_one(&app.pool)
            .await
            .unwrap();
        if remaining == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(remaining, 1);
    assert_eq!(app.join(&code, "Cyd").await.status(), 200);
}
