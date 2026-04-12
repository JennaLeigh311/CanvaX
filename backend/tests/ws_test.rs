// Integration test for websocket optimistic concurrency and session count broadcasts.
use axum::{Router, routing::get};
use canvax_backend::{
    config::Config,
    handlers,
    models::CanvasStateSnapshot,
    state::{AppState, SharedState},
    ws,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::{net::TcpListener, time::{Duration, Instant, timeout}};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use uuid::Uuid;

type TestWs = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

fn resolve_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| "postgres://user:password@127.0.0.1:5432/canvax".to_string())
}

async fn recv_json(ws: &mut TestWs, wait: Duration) -> Option<Value> {
    let message = timeout(wait, ws.next()).await.ok()??.ok()?;
    if let Message::Text(text) = message {
        serde_json::from_str::<Value>(&text).ok()
    } else {
        None
    }
}

async fn drain_messages(
    ws: &mut TestWs,
    latest_color: &mut Option<String>,
    joined_counts: &mut Vec<usize>,
    left_counts: &mut Vec<usize>,
    duration: Duration,
) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        let Some(value) = recv_json(ws, Duration::from_millis(100)).await else {
            continue;
        };

        let msg_type = value.get("type").and_then(Value::as_str);
        let payload = value.get("payload");

        match (msg_type, payload) {
            (Some("PixelAccepted"), Some(payload)) => {
                if let Some(color) = payload.get("color").and_then(Value::as_str) {
                    *latest_color = Some(color.to_string());
                }
            }
            (Some("PixelRejected"), Some(payload)) => {
                if let Some(color) = payload.get("winning_color").and_then(Value::as_str) {
                    *latest_color = Some(color.to_string());
                }
            }
            (Some("SessionJoined"), Some(payload)) => {
                if let Some(count) = payload
                    .get("active_session_count")
                    .and_then(Value::as_u64)
                {
                    joined_counts.push(count as usize);
                }
            }
            (Some("SessionLeft"), Some(payload)) => {
                if let Some(count) = payload
                    .get("active_session_count")
                    .and_then(Value::as_u64)
                {
                    left_counts.push(count as usize);
                }
            }
            _ => {}
        }
    }
}

#[tokio::test]
async fn websocket_clients_converge_and_receive_session_counts() {
    let database_url = resolve_database_url();
    let pool = PgPool::connect(&database_url)
        .await
        .unwrap_or_else(|error| panic!("failed to connect to test database '{database_url}': {error}"));

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .unwrap_or_else(|error| panic!("failed to run migrations in ws test: {error}"));

    let canvas_id = Uuid::new_v4();
    let canvas_name = format!("ws-test-canvas-{}", canvas_id);
    sqlx::query("INSERT INTO canvases (id, name, width, height) VALUES ($1, $2, $3, $4)")
        .bind(canvas_id)
        .bind(canvas_name)
        .bind(16_i32)
        .bind(16_i32)
        .execute(&pool)
        .await
        .expect("failed to create ws test canvas");

    let config = Config {
        database_url: database_url.clone(),
        host: "127.0.0.1".to_string(),
        port: 0,
        canvas_width: 64,
        canvas_height: 64,
        max_sessions: 500,
    };
    let state: SharedState = AppState::new(pool.clone(), config);

    let app = Router::new()
        .route("/", get(handlers::health_check))
        .nest("/api", handlers::routes())
        .route("/ws/canvas/{id}", get(ws::ws_handler))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind ws test listener");
    let addr = listener
        .local_addr()
        .expect("failed to read listener address");

    let server_handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let ws_url = format!("ws://{}/ws/canvas/{}", addr, canvas_id);

    let (mut ws1, _) = connect_async(ws_url.as_str())
        .await
        .expect("failed to connect ws client 1");
    let first_snapshot = recv_json(&mut ws1, Duration::from_secs(2))
        .await
        .expect("client 1 did not receive initial snapshot");
    serde_json::from_value::<CanvasStateSnapshot>(first_snapshot)
        .expect("client 1 first message was not a CanvasStateSnapshot");

    let (mut ws2, _) = connect_async(ws_url.as_str())
        .await
        .expect("failed to connect ws client 2");
    let second_snapshot = recv_json(&mut ws2, Duration::from_secs(2))
        .await
        .expect("client 2 did not receive initial snapshot");
    serde_json::from_value::<CanvasStateSnapshot>(second_snapshot)
        .expect("client 2 first message was not a CanvasStateSnapshot");

    let (mut ws3, _) = connect_async(ws_url.as_str())
        .await
        .expect("failed to connect ws client 3");
    let third_snapshot = recv_json(&mut ws3, Duration::from_secs(2))
        .await
        .expect("client 3 did not receive initial snapshot");
    serde_json::from_value::<CanvasStateSnapshot>(third_snapshot)
        .expect("client 3 first message was not a CanvasStateSnapshot");

    let mut c1_latest = None;
    let mut c2_latest = None;
    let mut c3_latest = None;
    let mut c1_joined = Vec::new();
    let mut c2_joined = Vec::new();
    let mut c3_joined = Vec::new();
    let mut c1_left = Vec::new();
    let mut c2_left = Vec::new();
    let mut c3_left = Vec::new();

    drain_messages(
        &mut ws1,
        &mut c1_latest,
        &mut c1_joined,
        &mut c1_left,
        Duration::from_millis(500),
    )
    .await;
    drain_messages(
        &mut ws2,
        &mut c2_latest,
        &mut c2_joined,
        &mut c2_left,
        Duration::from_millis(500),
    )
    .await;
    drain_messages(
        &mut ws3,
        &mut c3_latest,
        &mut c3_joined,
        &mut c3_left,
        Duration::from_millis(500),
    )
    .await;

    assert!(c1_joined.contains(&3) || c2_joined.contains(&3) || c3_joined.contains(&3));

    let client_ts = chrono::Utc::now().timestamp_millis() as u64;
    let msg1 = json!({"x": 4, "y": 7, "color": "#ff0000", "client_timestamp": client_ts, "session_id": "client-1"}).to_string();
    let msg2 = json!({"x": 4, "y": 7, "color": "#00ff00", "client_timestamp": client_ts, "session_id": "client-2"}).to_string();
    let msg3 = json!({"x": 4, "y": 7, "color": "#0000ff", "client_timestamp": client_ts, "session_id": "client-3"}).to_string();

    let (r1, r2, r3) = tokio::join!(
        ws1.send(Message::Text(msg1.into())),
        ws2.send(Message::Text(msg2.into())),
        ws3.send(Message::Text(msg3.into()))
    );
    r1.expect("client 1 failed to send update");
    r2.expect("client 2 failed to send update");
    r3.expect("client 3 failed to send update");

    let convergence_deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < convergence_deadline {
        drain_messages(
            &mut ws1,
            &mut c1_latest,
            &mut c1_joined,
            &mut c1_left,
            Duration::from_millis(150),
        )
        .await;
        drain_messages(
            &mut ws2,
            &mut c2_latest,
            &mut c2_joined,
            &mut c2_left,
            Duration::from_millis(150),
        )
        .await;
        drain_messages(
            &mut ws3,
            &mut c3_latest,
            &mut c3_joined,
            &mut c3_left,
            Duration::from_millis(150),
        )
        .await;

        if c1_latest.is_some() && c2_latest.is_some() && c3_latest.is_some() {
            break;
        }
    }

    let final1 = c1_latest
        .clone()
        .expect("client 1 did not observe a final color");
    let final2 = c2_latest
        .clone()
        .expect("client 2 did not observe a final color");
    let final3 = c3_latest
        .clone()
        .expect("client 3 did not observe a final color");

    assert_eq!(final1, final2);
    assert_eq!(final2, final3);

    ws2.close(None).await.expect("client 2 failed to close");
    ws3.close(None).await.expect("client 3 failed to close");

    drain_messages(
        &mut ws1,
        &mut c1_latest,
        &mut c1_joined,
        &mut c1_left,
        Duration::from_secs(2),
    )
    .await;

    assert!(c1_left.contains(&2));
    assert!(c1_left.contains(&1));

    ws1.close(None).await.expect("client 1 failed to close");

    sqlx::query("DELETE FROM canvases WHERE id = $1")
        .bind(canvas_id)
        .execute(&pool)
        .await
        .expect("failed to cleanup ws test canvas");

    server_handle.abort();
}
