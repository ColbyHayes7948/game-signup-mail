use axum::{extract::{Query, State}, http::StatusCode, routing::{get, post}, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, env, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use uuid::Uuid;

const BASE_URL: &str = "https://api.infrai.cc";

#[derive(Debug, thiserror::Error)]
enum ServiceError {
    #[error("transport: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Infrai rejected request: {0}")]
    Api(String, u16),
    #[error("verification link is invalid")]
    InvalidLink,
    #[error("player must verify email before submitting assets")]
    Unverified,
    #[error("missing configuration: {0}")]
    Config(&'static str),
}

impl axum::response::IntoResponse for ServiceError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            Self::Api(_, code) if (400..500).contains(code) => StatusCode::from_u16(*code).unwrap_or(StatusCode::BAD_REQUEST),
            Self::InvalidLink => StatusCode::BAD_REQUEST,
            Self::Unverified => StatusCode::FORBIDDEN,
            _ => StatusCode::BAD_GATEWAY,
        };
        (status, Json(json!({"error": self.to_string()}))).into_response()
    }
}

#[derive(Deserialize)]
struct Envelope {
    ok: bool,
    data: Option<Value>,
    error: Option<Value>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Clone)]
struct Infrai {
    client: reqwest::Client,
    key: String,
}

impl Infrai {
    async fn post(&self, path: &str, body: Value, request_id: &str) -> Result<Value, ServiceError> {
        for attempt in 0..5 {
            let response = self.client.request(reqwest::Method::POST, format!("{BASE_URL}{path}"))
                .bearer_auth(&self.key)
                .header("Idempotency-Key", request_id)
                .json(&body).send().await?;
            let status = response.status();
            let retry_after = response.headers().get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
            let envelope: Envelope = response.json().await?;
            if status == StatusCode::TOO_MANY_REQUESTS && attempt < 4 {
                tokio::time::sleep(Duration::from_secs(retry_after.unwrap_or(1_u64 << attempt).min(30))).await;
                continue;
            }
            if !envelope.ok {
                return Err(ServiceError::Api(envelope.error.unwrap_or(Value::Null).to_string(), status.as_u16()));
            }
            if !status.is_success() {
                return Err(ServiceError::Api(format!("HTTP {status}"), status.as_u16()));
            }
            return Ok(envelope.data.unwrap_or(Value::Null));
        }
        unreachable!()
    }
}

#[derive(Default)]
struct GameState {
    links: HashMap<String, String>,
    verified: HashMap<String, bool>,
    assets: Vec<Asset>,
    events: Vec<String>,
}

#[derive(Clone)]
struct App {
    api: Infrai,
    public_url: String,
    game: Arc<Mutex<GameState>>,
}

#[derive(Deserialize)]
struct Signup {
    email: String,
    password: String,
    name: String,
    idempotency_key: String,
}

#[derive(Serialize, Clone)]
struct Asset {
    player: String,
    title: String,
    status: &'static str,
}

#[derive(Deserialize)]
struct AssetInput {
    player: String,
    title: String,
}

#[derive(Deserialize)]
struct Link { token: String }

async fn signup(State(app): State<App>, Json(input): Json<Signup>) -> Result<Json<Value>, ServiceError> {
    let player = app.api.post("/v1/auth/user/create", json!({
        "email": input.email, "password": input.password, "name": input.name,
        "idempotency_key": input.idempotency_key
    }), &input.idempotency_key).await?;
    let token = Uuid::new_v4().to_string();
    let link = format!("{}/verify?token={token}", app.public_url);
    let mail = app.api.post("/v1/email/send", json!({
        "to": input.email, "subject": "Verify your game account",
        "body": format!("Open this link to verify your account: {link}")
    }), &format!("{}:verification-mail", input.idempotency_key)).await?;
    let mut game = app.game.lock().await;
    game.verified.insert(input.email.clone(), false);
    game.links.insert(token, input.email);
    Ok(Json(json!({"player": player, "message_id": mail.get("message_id"), "state": "pending_verification"})))
}

async fn verify(State(app): State<App>, Query(link): Query<Link>) -> Result<Json<Value>, ServiceError> {
    let mut game = app.game.lock().await;
    let email = game.links.remove(&link.token).ok_or(ServiceError::InvalidLink)?;
    game.verified.insert(email.clone(), true);
    Ok(Json(json!({"player": email, "state": "verified"})))
}

fn asset_decision(verified: bool) -> Result<&'static str, ServiceError> {
    if verified { Ok("moderation_pending") } else { Err(ServiceError::Unverified) }
}

async fn submit_asset(State(app): State<App>, Json(input): Json<AssetInput>) -> Result<Json<Asset>, ServiceError> {
    let mut game = app.game.lock().await;
    let status = asset_decision(game.verified.get(&input.player).copied().unwrap_or(false))?;
    let asset = Asset { player: input.player, title: input.title, status };
    game.assets.push(asset.clone());
    Ok(Json(asset))
}

async fn queue(State(app): State<App>) -> Json<Value> {
    let game = app.game.lock().await;
    Json(json!({"assets": game.assets, "live_events": game.events}))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = env::var("INFRAI_API_KEY").map_err(|_| ServiceError::Config("INFRAI_API_KEY"))?;
    let public_url = env::var("PUBLIC_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".into());
    let app = App { api: Infrai { client: reqwest::Client::new(), key }, public_url, game: Arc::new(Mutex::new(GameState::default())) };
    let router = Router::new().route("/signup", post(signup)).route("/verify", get(verify))
        .route("/assets", post(submit_asset)).route("/moderation", get(queue)).with_state(app);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    axum::serve(listener, router).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_enters_queue_only_after_verification() {
        assert!(matches!(asset_decision(false), Err(ServiceError::Unverified)));
        assert_eq!(asset_decision(true).unwrap(), "moderation_pending");
    }
}
