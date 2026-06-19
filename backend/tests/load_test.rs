// Long-running websocket load test for concurrency and throughput claims.
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
use std::{
    collections::HashMap,
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    net::TcpListener,
    task::JoinHandle,
    time::{sleep, timeout},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::Message,
};
use uuid::Uuid;

type TestWs = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Clone)]
struct SendEventMeta {
    sent_at: Instant,
    sender_index: usize,
}

fn resolve_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| "postgres://user:password@127.0.0.1:5432/canvax".to_string())
}

fn read_env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn read_env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn current_process_rss_bytes() -> Option<u64> {
    let pid = std::process::id().to_string();
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let rss_kb = String::from_utf8(output.stdout).ok()?.trim().parse::<u64>().ok()?;
    Some(rss_kb.saturating_mul(1024))
}

async fn recv_json(ws: &mut TestWs, wait: Duration) -> Option<Value> {
    let message = timeout(wait, ws.next()).await.ok()??.ok()?;
    match message {
        Message::Text(text) => serde_json::from_str::<Value>(&text).ok(),
        _ => None,
    }
}

fn update_peak_memory(peak_memory_bytes: &AtomicU64, candidate_bytes: u64) {
    let mut observed = peak_memory_bytes.load(Ordering::Relaxed);
    while candidate_bytes > observed {
        match peak_memory_bytes.compare_exchange(
            observed,
            candidate_bytes,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(current) => observed = current,
        }
    }
}

#[tokio::test]
#[ignore = "Long-running load test for 100+ concurrent websocket sessions"]
async fn websocket_load_test_100_clients_30_seconds() {
    let total_connections = read_env_usize("LOAD_TEST_CONNECTIONS", 100);
    let updates_per_second = read_env_u64("LOAD_TEST_UPDATES_PER_SECOND", 10);
    let duration_seconds = read_env_u64("LOAD_TEST_DURATION_SECONDS", 30);

    let database_url = resolve_database_url();
    let pool = PgPool::connect(&database_url)
        .await
        .unwrap_or_else(|error| panic!("failed to connect to test database '{database_url}': {error}"));

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .unwrap_or_else(|error| panic!("failed to run migrations in load test: {error}"));

    sqlx::query("DELETE FROM sessions")
        .execute(&pool)
        .await
        .expect("failed to clear sessions table");
    sqlx::query("DELETE FROM pixels")
        .execute(&pool)
        .await
        .expect("failed to clear pixels table");
    sqlx::query("DELETE FROM canvases")
        .execute(&pool)
        .await
        .expect("failed to clear canvases table");

    let canvas_id = Uuid::new_v4();
    sqlx::query("INSERT INTO canvases (id, name, width, height) VALUES ($1, $2, $3, $4)")
        .bind(canvas_id)
        .bind("Load Test Canvas")
        .bind(64_i32)
        .bind(64_i32)
        .execute(&pool)
        .await
        .expect("failed to create load test canvas");

    let config = Config {
        database_url: database_url.clone(),
        host: "127.0.0.1".to_string(),
        port: 0,
        canvas_width: 64,
        canvas_height: 64,
        max_sessions: 500,
        database_max_connections: 10,
        broadcast_capacity: 1024,
        pixel_write_buffer: 1024,
        pixel_flush_interval_ms: 50,
        pixel_flush_max_batch: 128,
    };
    let state: SharedState = AppState::new(pool.clone(), config);

    let app = Router::new()
        .route("/", get(handlers::health_check))
        .route("/health", get(handlers::deployment_health))
        .nest("/api", handlers::routes())
        .route("/ws/canvas/{id}", get(ws::ws_handler))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind load test listener");
    let addr = listener
        .local_addr()
        .expect("failed to read listener address");

    let server_handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let ws_url = format!("ws://{}/ws/canvas/{}", addr, canvas_id);

    let send_events: Arc<Mutex<HashMap<String, SendEventMeta>>> = Arc::new(Mutex::new(HashMap::new()));
    let latency_total_ns = Arc::new(AtomicU64::new(0));
    let latency_samples = Arc::new(AtomicU64::new(0));
    let conflict_count = Arc::new(AtomicU64::new(0));
    let connection_drops = Arc::new(AtomicU64::new(0));
    let peak_memory_bytes = Arc::new(AtomicU64::new(current_process_rss_bytes().unwrap_or(0)));
    let global_message_index = Arc::new(AtomicU64::new(0));
    let stop_receivers = Arc::new(AtomicBool::new(false));

    let stop_sampler = Arc::new(AtomicBool::new(false));
    let sampler_peak = peak_memory_bytes.clone();
    let sampler_stop = stop_sampler.clone();
    let sampler_handle = tokio::spawn(async move {
        while !sampler_stop.load(Ordering::Relaxed) {
            if let Some(bytes) = current_process_rss_bytes() {
                update_peak_memory(&sampler_peak, bytes);
            }
            sleep(Duration::from_millis(500)).await;
        }
    });

    let mut sender_handles: Vec<JoinHandle<()>> = Vec::with_capacity(total_connections);
    let mut receiver_handles: Vec<JoinHandle<()>> = Vec::with_capacity(total_connections);

    let test_start = Instant::now();
    let send_end = test_start + Duration::from_secs(duration_seconds);
    let send_interval = Duration::from_millis((1000 / updates_per_second.max(1)) as u64);

    for client_index in 0..total_connections {
        let (mut ws, _) = connect_async(ws_url.as_str())
            .await
            .unwrap_or_else(|error| panic!("failed to connect ws client {client_index}: {error}"));

        let snapshot = recv_json(&mut ws, Duration::from_secs(3))
            .await
            .unwrap_or_else(|| panic!("client {client_index} did not receive initial snapshot"));

        serde_json::from_value::<CanvasStateSnapshot>(snapshot)
            .unwrap_or_else(|error| panic!("client {client_index} initial message was not snapshot: {error}"));

        let (mut write_half, mut read_half) = ws.split();

        let send_events_write = send_events.clone();
        let global_message_index_write = global_message_index.clone();
        let drops_write = connection_drops.clone();
        let sender_handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(send_interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                ticker.tick().await;
                if Instant::now() >= send_end {
                    break;
                }

                let message_seq = global_message_index_write.fetch_add(1, Ordering::Relaxed);
                let color = format!("#{:06x}", (message_seq % 0x00FF_FFFF) as u32);

                {
                    let mut guard = send_events_write
                        .lock()
                        .expect("failed to lock send events map for write");
                    guard.insert(
                        color.clone(),
                        SendEventMeta {
                            sent_at: Instant::now(),
                            sender_index: client_index,
                        },
                    );
                }

                let x = ((message_seq + client_index as u64) % 8) as i32;
                let y = ((message_seq / 8 + client_index as u64) % 8) as i32;
                let payload = json!({
                    "x": x,
                    "y": y,
                    "color": color,
                    "client_timestamp": chrono::Utc::now().timestamp_millis() as u64,
                    "session_id": format!("load-test-client-{client_index}"),
                });

                if write_half
                    .send(Message::Text(payload.to_string().into()))
                    .await
                    .is_err()
                {
                    drops_write.fetch_add(1, Ordering::Relaxed);
                    break;
                }
            }
        });
        sender_handles.push(sender_handle);

        let send_events_read = send_events.clone();
        let latency_total_ns_read = latency_total_ns.clone();
        let latency_samples_read = latency_samples.clone();
        let conflicts_read = conflict_count.clone();
        let drops_read = connection_drops.clone();
        let stop_receivers_read = stop_receivers.clone();

        let receiver_handle = tokio::spawn(async move {
            loop {
                if stop_receivers_read.load(Ordering::Relaxed) {
                    break;
                }

                let next = timeout(Duration::from_millis(500), read_half.next()).await;
                let message = match next {
                    Ok(Some(Ok(message))) => message,
                    Ok(Some(Err(_))) | Ok(None) => {
                        drops_read.fetch_add(1, Ordering::Relaxed);
                        break;
                    }
                    Err(_) => {
                        continue;
                    }
                };

                let Message::Text(text) = message else {
                    continue;
                };

                let Ok(value) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };

                let msg_type = value.get("type").and_then(Value::as_str);
                let payload = value.get("payload").and_then(Value::as_object);

                match (msg_type, payload) {
                    (Some("PixelRejected"), Some(_)) => {
                        conflicts_read.fetch_add(1, Ordering::Relaxed);
                    }
                    (Some("PixelAccepted"), Some(payload)) => {
                        if let Some(color) = payload.get("color").and_then(Value::as_str) {
                            let maybe_meta = {
                                let guard = send_events_read
                                    .lock()
                                    .expect("failed to lock send events map for read");
                                guard.get(color).cloned()
                            };

                            if let Some(meta) = maybe_meta {
                                if meta.sender_index != client_index {
                                    let latency_ns = meta.sent_at.elapsed().as_nanos() as u64;
                                    latency_total_ns_read.fetch_add(latency_ns, Ordering::Relaxed);
                                    latency_samples_read.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        });
        receiver_handles.push(receiver_handle);
    }

    for sender in sender_handles {
        let _ = sender.await;
    }

    sleep(Duration::from_secs(2)).await;
    stop_receivers.store(true, Ordering::Relaxed);

    for receiver in receiver_handles {
        receiver.abort();
    }

    stop_sampler.store(true, Ordering::Relaxed);
    let _ = sampler_handle.await;

    let elapsed = test_start.elapsed();
    let samples = latency_samples.load(Ordering::Relaxed);
    let total_latency_ns = latency_total_ns.load(Ordering::Relaxed);
    let average_latency_ms = if samples == 0 {
        0.0
    } else {
        (total_latency_ns as f64 / samples as f64) / 1_000_000.0
    };

    let rejected = conflict_count.load(Ordering::Relaxed);
    let drops = connection_drops.load(Ordering::Relaxed);
    let peak_memory_mb = peak_memory_bytes.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0);

    println!(
        "\nLOAD TEST SUMMARY\n  connections: {total_connections}\n  updates_per_second_per_client: {updates_per_second}\n  duration_seconds: {duration_seconds}\n  elapsed_seconds: {:.2}\n  avg_broadcast_latency_ms: {:.2}\n  pixel_rejected_conflicts: {}\n  connection_drops: {}\n  peak_memory_mb: {:.2}\n",
        elapsed.as_secs_f64(),
        average_latency_ms,
        rejected,
        drops,
        peak_memory_mb,
    );

    assert!(samples > 0, "no latency samples were captured");

    server_handle.abort();
}
