use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, Json},
    routing::{get, post},
    Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::Rng;
use serde::{Deserialize, Serialize};
use spinwin_core::{sign_ticket, verify_ticket, TicketPayload};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

struct AppState {
    db: SqlitePool,
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

#[derive(Serialize, Clone)]
struct PrizeInfo {
    id: i64,
    name: String,
    image_url: String,
    total_qty: i64,
    remaining: i64,
}

#[derive(Serialize)]
struct SpinResult {
    prize: PrizeInfo,
    angle: f64,
}

#[derive(Deserialize)]
struct ClaimRequest {
    name: String,
    email: String,
    prize_id: i64,
}

#[derive(Serialize)]
struct ClaimResponse {
    ticket_id: String,
    qr_data: String,
    prize_name: String,
    attendee_name: String,
}

#[derive(Serialize)]
struct VerifyResponse {
    valid: bool,
    prize: Option<String>,
    attendee: Option<String>,
    redeemed: Option<bool>,
}

#[derive(Serialize)]
struct RedeemResponse {
    success: bool,
    message: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

fn db_err(e: sqlx::Error) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: format!("Database error: {}", e),
        }),
    )
}

async fn get_prizes(State(state): State<Arc<AppState>>) -> Json<Vec<PrizeInfo>> {
    let rows = sqlx::query("SELECT id, name, image_url, total_qty, remaining FROM prizes")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    let prizes: Vec<PrizeInfo> = rows
        .iter()
        .map(|r| PrizeInfo {
            id: r.get("id"),
            name: r.get("name"),
            image_url: r.get("image_url"),
            total_qty: r.get("total_qty"),
            remaining: r.get("remaining"),
        })
        .collect();

    Json(prizes)
}

async fn check_email(
    State(state): State<Arc<AppState>>,
    Path(email): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let existing = sqlx::query("SELECT id FROM tickets WHERE email = ?")
        .bind(&email)
        .fetch_optional(&state.db)
        .await
        .map_err(db_err)?;

    Ok(Json(serde_json::json!({
        "already_played": existing.is_some()
    })))
}

async fn spin(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SpinResult>, (StatusCode, Json<ErrorResponse>)> {
    let rows =
        sqlx::query("SELECT id, name, image_url, total_qty, remaining FROM prizes WHERE remaining > 0")
            .fetch_all(&state.db)
            .await
            .map_err(db_err)?;

    let prizes: Vec<PrizeInfo> = rows
        .iter()
        .map(|r| PrizeInfo {
            id: r.get("id"),
            name: r.get("name"),
            image_url: r.get("image_url"),
            total_qty: r.get("total_qty"),
            remaining: r.get("remaining"),
        })
        .collect();

    if prizes.is_empty() {
        return Err((
            StatusCode::GONE,
            Json(ErrorResponse {
                error: "All prizes have been claimed!".to_string(),
            }),
        ));
    }

    // Weighted random selection based on remaining quantities
    let total_remaining: i64 = prizes.iter().map(|p| p.remaining).sum();
    let mut rng = rand::thread_rng();
    let roll = rng.gen_range(0..total_remaining);

    let mut cumulative = 0i64;
    let mut selected_idx = 0;
    for (i, prize) in prizes.iter().enumerate() {
        cumulative += prize.remaining;
        if roll < cumulative {
            selected_idx = i;
            break;
        }
    }

    let selected = &prizes[selected_idx];

    // Calculate landing angle for the wheel animation.
    // The wheel draws segments clockwise from the top (offset -90°). The pointer is at top.
    // When the wheel rotates by R degrees, the pointer reads the segment at position (360 - R%360).
    // So to land on a segment starting at `segment_start`, we need R%360 = 360 - segment_start - offset.
    let segment_start: f64 = prizes[..selected_idx]
        .iter()
        .map(|p| p.remaining as f64 / total_remaining as f64 * 360.0)
        .sum();
    let segment_size = selected.remaining as f64 / total_remaining as f64 * 360.0;
    let angle_within_segment = rng.gen_range(0.2..0.8) * segment_size;
    let landing_angle = 360.0 - (segment_start + angle_within_segment);

    // Add full rotations for visual effect
    let full_rotations = rng.gen_range(5..8) as f64 * 360.0;
    let final_angle = full_rotations + landing_angle;

    Ok(Json(SpinResult {
        prize: selected.clone(),
        angle: final_angle,
    }))
}

async fn claim(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ClaimRequest>,
) -> Result<Json<ClaimResponse>, (StatusCode, Json<ErrorResponse>)> {
    let email = req.email.trim().to_lowercase();
    let name = req.name.trim().to_string();

    if email.is_empty() || name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Name and email are required".to_string(),
            }),
        ));
    }

    // Check if email already used
    let existing = sqlx::query("SELECT id FROM tickets WHERE email = ?")
        .bind(&email)
        .fetch_optional(&state.db)
        .await
        .map_err(db_err)?;

    if existing.is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "This email has already been used to claim a prize".to_string(),
            }),
        ));
    }

    // Atomically decrement prize stock
    let result = sqlx::query("UPDATE prizes SET remaining = remaining - 1 WHERE id = ? AND remaining > 0")
        .bind(req.prize_id)
        .execute(&state.db)
        .await
        .map_err(db_err)?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::GONE,
            Json(ErrorResponse {
                error: "This prize is no longer available".to_string(),
            }),
        ));
    }

    // Get prize name
    let prize_row = sqlx::query("SELECT name FROM prizes WHERE id = ?")
        .bind(req.prize_id)
        .fetch_one(&state.db)
        .await
        .map_err(db_err)?;
    let prize_name: String = prize_row.get("name");

    let ticket_id = uuid::Uuid::new_v4().to_string();

    let payload = TicketPayload {
        ticket_id: ticket_id.clone(),
        email: email.clone(),
        name: name.clone(),
        prize_name: prize_name.clone(),
        prize_id: req.prize_id,
    };

    let qr_data = sign_ticket(&state.signing_key, &payload);

    // Store ticket
    let insert_result = sqlx::query(
        "INSERT INTO tickets (id, email, name, prize_id, token, redeemed) VALUES (?, ?, ?, ?, ?, FALSE)",
    )
    .bind(&ticket_id)
    .bind(&email)
    .bind(&name)
    .bind(req.prize_id)
    .bind(&qr_data)
    .execute(&state.db)
    .await;

    if let Err(e) = insert_result {
        // Restore prize stock on ticket creation failure
        let _ = sqlx::query("UPDATE prizes SET remaining = remaining + 1 WHERE id = ?")
            .bind(req.prize_id)
            .execute(&state.db)
            .await;

        if e.to_string().contains("UNIQUE") {
            return Err((
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: "This email has already been used to claim a prize".to_string(),
                }),
            ));
        }
        return Err(db_err(e));
    }

    Ok(Json(ClaimResponse {
        ticket_id,
        qr_data,
        prize_name,
        attendee_name: name,
    }))
}

async fn verify_handler(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Json<VerifyResponse> {
    match verify_ticket(&state.verifying_key, &token) {
        Ok(result) if result.valid => {
            // Check redemption status from DB
            let row = sqlx::query("SELECT redeemed FROM tickets WHERE token = ?")
                .bind(&token)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten();

            let redeemed = row.map(|r| {
                let v: bool = r.get("redeemed");
                v
            });

            Json(VerifyResponse {
                valid: true,
                prize: Some(result.payload.prize_name),
                attendee: Some(result.payload.name),
                redeemed,
            })
        }
        _ => Json(VerifyResponse {
            valid: false,
            prize: None,
            attendee: None,
            redeemed: None,
        }),
    }
}

async fn redeem(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Result<Json<RedeemResponse>, (StatusCode, Json<ErrorResponse>)> {
    // First verify the signature
    match verify_ticket(&state.verifying_key, &token) {
        Ok(result) if result.valid => {}
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid ticket".to_string(),
                }),
            ));
        }
    }

    // Atomically mark as redeemed
    let result =
        sqlx::query("UPDATE tickets SET redeemed = TRUE WHERE token = ? AND redeemed = FALSE")
            .bind(&token)
            .execute(&state.db)
            .await
            .map_err(db_err)?;

    if result.rows_affected() == 0 {
        return Ok(Json(RedeemResponse {
            success: false,
            message: "Ticket already redeemed".to_string(),
        }));
    }

    Ok(Json(RedeemResponse {
        success: true,
        message: "Prize redeemed successfully!".to_string(),
    }))
}

async fn get_public_key(State(state): State<Arc<AppState>>) -> String {
    URL_SAFE_NO_PAD.encode(state.verifying_key.to_bytes())
}

async fn init_db(pool: &SqlitePool) {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS prizes (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            image_url TEXT NOT NULL,
            total_qty INTEGER NOT NULL,
            remaining INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await
    .expect("create prizes table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS tickets (
            id TEXT PRIMARY KEY,
            email TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            prize_id INTEGER NOT NULL REFERENCES prizes(id),
            token TEXT NOT NULL,
            redeemed BOOLEAN NOT NULL DEFAULT FALSE,
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(pool)
    .await
    .expect("create tickets table");

    // Seed prizes if table is empty
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM prizes")
        .fetch_one(pool)
        .await
        .expect("count prizes");

    if count.0 == 0 {
        let prizes = vec![
            ("Necklace", "necklace.svg", 100),
            ("Ring", "ring.svg", 200),
            ("Jewelry Set", "jewelry_set.svg", 50),
            ("Earring", "earring.svg", 50),
            ("Bangles", "bangles.svg", 50),
        ];
        for (name, image, qty) in prizes {
            sqlx::query(
                "INSERT INTO prizes (name, image_url, total_qty, remaining) VALUES (?, ?, ?, ?)",
            )
            .bind(name)
            .bind(image)
            .bind(qty)
            .bind(qty)
            .execute(pool)
            .await
            .expect("seed prize");
        }
        tracing::info!("Seeded 5 prizes");
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Signing key from env or generate deterministic one for dev
    let seed_hex = std::env::var("SPINWIN_SIGNING_KEY").unwrap_or_else(|_| {
        tracing::warn!("No SPINWIN_SIGNING_KEY set, using dev key — DO NOT use in production");
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string()
    });

    let seed_bytes: Vec<u8> = (0..seed_hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&seed_hex[i..i + 2], 16).expect("valid hex"))
        .collect();
    let seed: [u8; 32] = seed_bytes.try_into().expect("seed must be 32 bytes");

    let (signing_key, verifying_key) = spinwin_core::keypair_from_seed(&seed);

    let db_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:spinwin.db?mode=rwc".to_string());

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("connect to database");

    init_db(&pool).await;

    let state = Arc::new(AppState {
        db: pool,
        signing_key,
        verifying_key,
    });

    let api = Router::new()
        .route("/api/prizes", get(get_prizes))
        .route("/api/spin", post(spin))
        .route("/api/claim", post(claim))
        .route("/api/verify/{token}", get(verify_handler))
        .route("/api/redeem/{token}", post(redeem))
        .route("/api/public-key", get(get_public_key))
        .route("/api/check-email/{email}", get(check_email))
        .with_state(state)
        .layer(CorsLayer::permissive());

    // Clean URL routes serving HTML files
    let api = api.route(
        "/scan",
        get(|| async { Html(include_str!("../frontend/scan.html")) }),
    );

    let app = api.fallback_service(ServeDir::new("frontend"));

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    tracing::info!("Listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    axum::serve(listener, app).await.expect("server error");
}
