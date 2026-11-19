//! M2 "done" scenario over real sockets: two players solve Lock & Manual by combining what each
//! of them sees (the test plays the part of the kids talking), and the room completes.

mod common;

use serde_json::{Value, json};
use sqlx::PgPool;

use common::{TestApp, WsClient};

struct Game {
    host: WsClient,
    lock: WsClient,
    manual: WsClient,
    lock_view: Value,
    manual_view: Value,
    session_id: String,
}

/// Host + two players, both ready, game started. Seat 0 (Ali) holds the lock on puzzle 1.
async fn start_game(app: &TestApp) -> Game {
    let (_, session_id, code, host_token) = app.host_with_session("host@x.com").await;
    let mut host = app.ws(&host_token).await;
    host.recv_type("welcome").await;
    let mut ali = app.ws(&app.join_ok(&code, "Ali").await).await;
    ali.recv_type("welcome").await;
    let mut bea = app.ws(&app.join_ok(&code, "Bea").await).await;
    bea.recv_type("welcome").await;

    for p in [&mut ali, &mut bea] {
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

    let lock_view = ali.recv_type("view_update").await["data"].clone();
    let manual_view = bea.recv_type("view_update").await["data"].clone();
    Game {
        host,
        lock: ali,
        manual: bea,
        lock_view,
        manual_view,
        session_id,
    }
}

/// What the kids work out by talking: the manual reader reads each rule, the lock holder
/// reports the numbers of that color, and they do the arithmetic together.
fn work_out_code(lock_view: &Value, manual_view: &Value) -> Vec<u8> {
    let tiles = lock_view["view"]["view"]["tiles"].as_array().unwrap();
    let rules = manual_view["view"]["view"]["rules"].as_array().unwrap();
    rules
        .iter()
        .map(|rule| {
            let numbers: Vec<u64> = tiles
                .iter()
                .filter(|t| t["color"] == rule["color"])
                .map(|t| t["number"].as_u64().unwrap())
                .collect();
            let value = match rule["kind"].as_str().unwrap() {
                "sum" => numbers.iter().sum(),
                "count" => numbers.len() as u64,
                "largest" => *numbers.iter().max().unwrap(),
                "smallest" => *numbers.iter().min().unwrap(),
                other => panic!("unknown rule kind {other}"),
            };
            u8::try_from(value).unwrap()
        })
        .collect()
}

fn submit(index: u64, code: &[u8]) -> Value {
    json!({
        "type": "puzzle_action",
        "data": {
            "puzzle_index": index,
            "action": {"kind": "lock_and_manual", "action": {"type": "submit", "code": code}}
        }
    })
}

#[sqlx::test]
async fn two_players_solve_lock_and_manual_together(pool: PgPool) {
    let app = common::spawn(pool).await;
    let mut g = start_game(&app).await;

    // Each player sees only their half.
    assert_eq!(g.lock_view["progress"], json!({"index": 0, "total": 1}));
    assert_eq!(g.lock_view["view"]["view"]["role"], "lock");
    assert_eq!(g.manual_view["view"]["view"]["role"], "manual");
    assert!(g.lock_view["view"]["view"].get("rules").is_none());
    assert!(g.manual_view["view"]["view"].get("tiles").is_none());

    let code = work_out_code(&g.lock_view, &g.manual_view);

    // The manual reader can't type the code in.
    g.manual.send(submit(0, &code)).await;
    assert_eq!(
        g.manual.recv_type("error").await["data"]["code"],
        "invalid_action"
    );

    // A wrong guess: everyone hears "wrong" (by Ali), nobody learns the answer.
    let wrong: Vec<u8> = code
        .iter()
        .map(|d| if *d == 9 { 1 } else { d + 1 })
        .collect();
    g.lock.send(submit(0, &wrong)).await;
    for socket in [&mut g.host, &mut g.lock, &mut g.manual] {
        let result = socket.recv_type("action_result").await;
        assert_eq!(
            result["data"],
            json!({"puzzle_index": 0, "outcome": "wrong", "by": "Ali"})
        );
    }
    let manual_after = g.manual.recv_type("view_update").await;
    assert_eq!(
        manual_after["data"]["view"]["view"]["wrong_attempts"],
        json!([wrong])
    );

    // The right code opens the lock and ends the game for everyone.
    g.lock.send(submit(0, &code)).await;
    for socket in [&mut g.host, &mut g.lock, &mut g.manual] {
        assert_eq!(
            socket.recv_type("action_result").await["data"]["outcome"],
            "solved"
        );
        let solved = socket.recv_type("puzzle_solved").await;
        assert_eq!(
            solved["data"],
            json!({"puzzle_index": 0, "next_index": null})
        );
        let done = socket.recv_type("room_completed").await;
        assert_eq!(done["data"]["escaped"], true);
    }

    // Persisted as escaped, with an end time.
    let id: uuid::Uuid = g.session_id.parse().unwrap();
    let mut status = String::new();
    for _ in 0..50 {
        let row = sqlx::query!(
            r#"SELECT status::text AS "status!", ended_at FROM game_sessions WHERE id = $1"#,
            id
        )
        .fetch_one(&app.pool)
        .await
        .unwrap();
        status = row.status;
        if status == "escaped" {
            assert!(row.ended_at.is_some());
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(status, "escaped");
}

#[sqlx::test]
async fn reconnect_mid_puzzle_restores_the_view(pool: PgPool) {
    let app = common::spawn(pool).await;
    let g = start_game(&app).await;
    let Game {
        manual,
        manual_view,
        ..
    } = g;
    drop(manual);

    // Bea's tab closed (token lost): she rejoins by nickname and gets her manual back.
    let code = sqlx::query_scalar!("SELECT code FROM game_sessions")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let mut bea = app.ws(&app.join_ok(&code, "Bea").await).await;
    let welcome = bea.recv_type("welcome").await;
    assert_eq!(welcome["data"]["status"], "running");
    assert_eq!(welcome["data"]["progress"], json!({"index": 0, "total": 1}));
    assert_eq!(welcome["data"]["view"], manual_view["view"]);
}

#[sqlx::test]
async fn malformed_puzzle_actions_are_rejected(pool: PgPool) {
    let app = common::spawn(pool).await;
    let mut g = start_game(&app).await;

    // Wrong number of digits: an error for the sender, not a wrong attempt for the team.
    g.lock.send(submit(0, &[1, 2])).await;
    assert_eq!(
        g.lock.recv_type("error").await["data"]["code"],
        "invalid_action"
    );
    // A stale puzzle index.
    g.lock.send(submit(5, &[1, 2, 3])).await;
    assert_eq!(
        g.lock.recv_type("error").await["data"]["code"],
        "stale_puzzle"
    );
    // An action for a puzzle kind that doesn't exist.
    g.lock
        .send(json!({"type": "puzzle_action", "data": {"puzzle_index": 0, "action": {"kind": "nope", "action": {}}}}))
        .await;
    assert_eq!(
        g.lock.recv_type("error").await["data"]["code"],
        "bad_message"
    );
}
