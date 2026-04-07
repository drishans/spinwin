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
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

struct SmtpConfig {
    email: String,
    password: String,
}

struct AppState {
    db: SqlitePool,
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
    registered_emails: RwLock<HashSet<String>>,
    smtp: Option<SmtpConfig>,
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

async fn fetch_registered_emails(sheet_id: &str) -> Result<HashSet<String>, String> {
    let url = format!(
        "https://docs.google.com/spreadsheets/d/{}/gviz/tq?tqx=out:csv",
        sheet_id
    );
    let body = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to fetch sheet: {}", e))?
        .text()
        .await
        .map_err(|e| format!("Failed to read sheet body: {}", e))?;

    let mut emails = HashSet::new();
    let mut reader = csv::Reader::from_reader(body.as_bytes());
    for result in reader.records() {
        if let Ok(record) = result {
            // Column B is index 1
            if let Some(email) = record.get(1) {
                let email = email.trim().to_lowercase();
                if !email.is_empty() && email.contains('@') {
                    emails.insert(email);
                }
            }
        }
    }
    Ok(emails)
}

fn spawn_email_refresh(state: Arc<AppState>, sheet_id: String) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            match fetch_registered_emails(&sheet_id).await {
                Ok(emails) => {
                    let count = emails.len();
                    *state.registered_emails.write().await = emails;
                    tracing::info!("Refreshed registered emails: {} entries", count);
                }
                Err(e) => {
                    tracing::error!("Failed to refresh emails: {}", e);
                }
            }
        }
    });
}

async fn check_email(
    State(state): State<Arc<AppState>>,
    Path(email): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let email = email.trim().to_lowercase();

    // Check if email is in the registered list
    let registered = state.registered_emails.read().await;
    if !registered.is_empty() && !registered.contains(&email) {
        return Ok(Json(serde_json::json!({
            "already_played": false,
            "not_registered": true
        })));
    }
    drop(registered);

    let existing = sqlx::query("SELECT id, name, token, prize_id FROM tickets WHERE email = ?")
        .bind(&email)
        .fetch_optional(&state.db)
        .await
        .map_err(db_err)?;

    match existing {
        Some(row) => {
            let ticket_id: String = row.get("id");
            let attendee_name: String = row.get("name");
            let qr_data: String = row.get("token");
            let prize_id: i64 = row.get("prize_id");

            let prize_name: String = sqlx::query("SELECT name FROM prizes WHERE id = ?")
                .bind(prize_id)
                .fetch_one(&state.db)
                .await
                .map_err(db_err)?
                .get("name");

            Ok(Json(serde_json::json!({
                "already_played": true,
                "not_registered": false,
                "ticket": {
                    "ticket_id": ticket_id,
                    "qr_data": qr_data,
                    "prize_name": prize_name,
                    "attendee_name": attendee_name
                }
            })))
        }
        None => {
            Ok(Json(serde_json::json!({
                "already_played": false,
                "not_registered": false
            })))
        }
    }
}

#[derive(Deserialize)]
struct SpinRequest {
    email: String,
}

async fn spin(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SpinRequest>,
) -> Result<Json<SpinResult>, (StatusCode, Json<ErrorResponse>)> {
    let email = req.email.trim().to_lowercase();

    // Check if email is registered
    let registered = state.registered_emails.read().await;
    if !registered.is_empty() && !registered.contains(&email) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "This email is not registered for the event".to_string(),
            }),
        ));
    }
    drop(registered);

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
                error: "This email has already been used".to_string(),
            }),
        ));
    }

    let rows =
        sqlx::query("SELECT id, name, image_url, total_qty, remaining FROM prizes WHERE remaining > 0")
            .fetch_all(&state.db)
            .await
            .map_err(db_err)?;

    let mut prizes: Vec<PrizeInfo> = rows
        .iter()
        .map(|r| PrizeInfo {
            id: r.get("id"),
            name: r.get("name"),
            image_url: r.get("image_url"),
            total_qty: r.get("total_qty"),
            remaining: r.get("remaining"),
        })
        .collect();

    // If only Mystery Prize (or nothing) has stock, fall back to unlimited mystery
    let non_mystery: Vec<&PrizeInfo> = prizes.iter().filter(|p| p.name != "Mystery Prize").collect();
    if non_mystery.is_empty() {
        // Get mystery prize info (even if remaining is 0)
        let mystery_row = sqlx::query("SELECT id, name, image_url, total_qty, remaining FROM prizes WHERE name = 'Mystery Prize'")
            .fetch_optional(&state.db)
            .await
            .map_err(db_err)?;

        match mystery_row {
            Some(r) => {
                prizes = vec![PrizeInfo {
                    id: r.get("id"),
                    name: r.get("name"),
                    image_url: r.get("image_url"),
                    total_qty: r.get("total_qty"),
                    remaining: 1, // virtual stock for selection
                }];
            }
            None => {
                return Err((
                    StatusCode::GONE,
                    Json(ErrorResponse {
                        error: "All prizes have been claimed!".to_string(),
                    }),
                ));
            }
        }
    }

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
    // The wheel uses equal-sized segments (probability is handled server-side).
    // The pointer is at the top (12 o'clock). When the wheel rotates by R degrees,
    // the pointer reads the segment at position (360 - R%360) % 360.
    let num_prizes = prizes.len() as f64;
    let segment_size = 360.0 / num_prizes;
    let segment_start = selected_idx as f64 * segment_size;
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
        // Check if this is Mystery Prize in fallback mode (all other prizes exhausted)
        let is_mystery: bool = sqlx::query("SELECT name FROM prizes WHERE id = ?")
            .bind(req.prize_id)
            .fetch_optional(&state.db)
            .await
            .map_err(db_err)?
            .map(|r| {
                let name: String = r.get("name");
                name == "Mystery Prize"
            })
            .unwrap_or(false);

        let others_exhausted: bool = sqlx::query("SELECT COUNT(*) as cnt FROM prizes WHERE name != 'Mystery Prize' AND remaining > 0")
            .fetch_one(&state.db)
            .await
            .map_err(db_err)?
            .get::<i64, _>("cnt") == 0;

        if !(is_mystery && others_exhausted) {
            return Err((
                StatusCode::GONE,
                Json(ErrorResponse {
                    error: "This prize is no longer available".to_string(),
                }),
            ));
        }
        // Mystery Prize in fallback mode — allow claim without stock decrement
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

    // Send ticket email in the background (don't block the response)
    if let Some(smtp) = &state.smtp {
        let smtp_email = smtp.email.clone();
        let smtp_password = smtp.password.clone();
        let to = email.clone();
        let aname = name.clone();
        let pname = prize_name.clone();
        let qr = qr_data.clone();
        tokio::spawn(async move {
            let cfg = SmtpConfig { email: smtp_email, password: smtp_password };
            send_ticket_email(&cfg, &to, &aname, &pname, &qr).await;
        });
    }

    Ok(Json(ClaimResponse {
        ticket_id,
        qr_data,
        prize_name,
        attendee_name: name,
    }))
}

async fn send_ticket_email(smtp: &SmtpConfig, to_email: &str, attendee_name: &str, prize_name: &str, qr_data: &str) {
    use lettre::{
        message::{header::ContentType, Attachment, MultiPart, SinglePart},
        transport::smtp::authentication::Credentials,
        AsyncSmtpTransport, AsyncTransport, Message,
    };

    // Generate QR code as PNG bytes
    let qr_png = match qrcode::QrCode::new(qr_data.as_bytes()) {
        Ok(code) => {
            let img = code.render::<image::Luma<u8>>().quiet_zone(true).min_dimensions(300, 300).build();
            let mut buf = std::io::Cursor::new(Vec::new());
            if img.write_to(&mut buf, image::ImageFormat::Png).is_err() {
                tracing::error!("Failed to encode QR PNG for {}", to_email);
                return;
            }
            buf.into_inner()
        }
        Err(e) => {
            tracing::error!("Failed to generate QR code for {}: {}", to_email, e);
            return;
        }
    };

    let html_body = format!(
        r#"<div style="font-family:sans-serif;max-width:480px;margin:0 auto;text-align:center;">
        <h2 style="color:#7b2d8e;">Spin & Win — WomenNowTV Sari Parade</h2>
        <p>Hi <strong>{}</strong>,</p>
        <p>You won a <strong style="color:#f9d423;">{}</strong>!</p>
        <p>Present this QR code at the Sari Parade booth to collect your prize:</p>
        <p><img src="cid:ticket-qr" width="250" height="250" alt="QR Ticket" /></p>
        <p style="color:#888;font-size:0.85rem;">Each code is single-use and cannot be shared.</p>
        </div>"#,
        attendee_name, prize_name
    );

    let qr_attachment = Attachment::new_inline("ticket-qr".to_string())
        .body(qr_png, ContentType::parse("image/png").unwrap());

    let email = match Message::builder()
        .from(smtp.email.parse().unwrap())
        .to(to_email.parse().unwrap())
        .subject(format!("Your Spin & Win Prize: {}", prize_name))
        .multipart(
            MultiPart::related()
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_HTML)
                        .body(html_body),
                )
                .singlepart(qr_attachment),
        ) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("Failed to build email for {}: {}", to_email, e);
            return;
        }
    };

    let creds = Credentials::new(smtp.email.clone(), smtp.password.clone());
    let mailer = match AsyncSmtpTransport::<lettre::Tokio1Executor>::relay("smtp.gmail.com") {
        Ok(builder) => builder.credentials(creds).build(),
        Err(e) => {
            tracing::error!("Failed to create SMTP transport: {}", e);
            return;
        }
    };

    match mailer.send(email).await {
        Ok(_) => tracing::info!("Ticket email sent to {}", to_email),
        Err(e) => tracing::error!("Failed to send email to {}: {}", to_email, e),
    }
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

async fn resend_ticket(
    State(state): State<Arc<AppState>>,
    Path(email): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let email = email.trim().to_lowercase();

    let row = sqlx::query("SELECT t.name, t.token, t.prize_id, p.name as prize_name FROM tickets t JOIN prizes p ON t.prize_id = p.id WHERE t.email = ?")
        .bind(&email)
        .fetch_optional(&state.db)
        .await
        .map_err(db_err)?;

    match row {
        Some(r) => {
            let attendee_name: String = r.get("name");
            let qr_data: String = r.get("token");
            let prize_name: String = r.get("prize_name");

            if let Some(smtp) = &state.smtp {
                let smtp_email = smtp.email.clone();
                let smtp_password = smtp.password.clone();
                let to = email.clone();
                let aname = attendee_name.clone();
                let pname = prize_name.clone();
                let qr = qr_data.clone();
                tokio::spawn(async move {
                    let cfg = SmtpConfig { email: smtp_email, password: smtp_password };
                    send_ticket_email(&cfg, &to, &aname, &pname, &qr).await;
                });
            }

            Ok(Json(serde_json::json!({ "sent": true })))
        }
        None => {
            Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "No ticket found for this email".to_string(),
                }),
            ))
        }
    }
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
        let small_stock = std::env::var("SPINWIN_SMALL_STOCK").is_ok();
        let prizes: Vec<(&str, &str, i64)> = if small_stock {
            tracing::info!("Using small stock quantities (test mode)");
            vec![
                ("Necklace", "necklace.jpg", 3),
                ("Ring", "ring.jpg", 5),
                ("Jewelry Set", "jewelry_set.jpg", 2),
                ("Earring", "earring.jpg", 2),
                ("Bangles", "bangles2.jpg", 2),
                ("Mystery Prize", "mystery.svg", 2),
            ]
        } else {
            vec![
                ("Necklace", "necklace.jpg", 100),
                ("Ring", "ring.jpg", 200),
                ("Jewelry Set", "jewelry_set.jpg", 50),
                ("Earring", "earring.jpg", 50),
                ("Bangles", "bangles2.jpg", 50),
                ("Mystery Prize", "mystery.svg", 10),
            ]
        };
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
    // Load .env from the project root (parent of server/)
    dotenvy::from_filename("../.env").ok();

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

    // Load registered emails from Google Sheet
    let sheet_id = std::env::var("GOOGLE_SHEET_ID")
        .ok()
        .filter(|s| !s.is_empty() && s != "none");
    let initial_emails = match &sheet_id {
        Some(id) => match fetch_registered_emails(id).await {
            Ok(emails) => {
                tracing::info!("Loaded {} registered emails from Google Sheet", emails.len());
                emails
            }
            Err(e) => {
                tracing::error!("Failed to load emails from Google Sheet: {}", e);
                HashSet::new()
            }
        },
        None => {
            tracing::warn!("No GOOGLE_SHEET_ID set — all emails will be allowed");
            HashSet::new()
        }
    };

    // SMTP config for sending ticket emails
    let smtp = match (std::env::var("SMTP_EMAIL"), std::env::var("SMTP_PASSWORD")) {
        (Ok(email), Ok(password)) => {
            tracing::info!("SMTP configured — ticket emails will be sent via {}", email);
            Some(SmtpConfig { email, password })
        }
        _ => {
            tracing::warn!("SMTP_EMAIL/SMTP_PASSWORD not set — ticket emails disabled");
            None
        }
    };

    let state = Arc::new(AppState {
        db: pool,
        signing_key,
        verifying_key,
        registered_emails: RwLock::new(initial_emails),
        smtp,
    });

    // Start background refresh for registered emails
    if let Some(id) = sheet_id {
        spawn_email_refresh(state.clone(), id);
    }

    let api = Router::new()
        .route("/api/prizes", get(get_prizes))
        .route("/api/spin", post(spin))
        .route("/api/claim", post(claim))
        .route("/api/verify/{token}", get(verify_handler))
        .route("/api/redeem/{token}", post(redeem))
        .route("/api/public-key", get(get_public_key))
        .route("/api/check-email/{email}", get(check_email))
        .route("/api/resend/{email}", post(resend_ticket))
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
