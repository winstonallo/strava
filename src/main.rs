use axum::{Json, Router, extract::Query, http::StatusCode, routing::get};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

const API: &str = "https://www.strava.com/api/v3";
// ponytail: plain file so rotated refresh tokens survive restarts; seed it once by hand
const REFRESH_TOKEN_FILE: &str = "refresh_token";

fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("{key} not set"))
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/webhook", get(verify).post(event));
    let listener = tokio::net::TcpListener::bind("[::]:3000").await.unwrap();
    // Strava hits GET /webhook during registration, so register only once we're listening.
    tokio::spawn(register());
    axum::serve(listener, app).await.unwrap();
}

async fn register() {
    let res = reqwest::Client::new()
        .post(format!("{API}/push_subscriptions"))
        .form(&[
            ("client_id", env("STRAVA_CLIENT_ID")),
            ("client_secret", env("STRAVA_CLIENT_SECRET")),
            ("callback_url", format!("https://{}/webhook", env("DOMAIN"))),
            ("verify_token", env("STRAVA_VERIFY_TOKEN")),
        ])
        .send()
        .await;
    // A 400 "already exists" on restart is expected: Strava allows one subscription per app.
    match res {
        Ok(r) => eprintln!("subscription: {} {}", r.status(), r.text().await.unwrap_or_default()),
        Err(e) => eprintln!("subscription failed: {e}"),
    }
}

async fn verify(Query(q): Query<HashMap<String, String>>) -> Result<Json<Value>, StatusCode> {
    if q.get("hub.verify_token") != Some(&env("STRAVA_VERIFY_TOKEN")) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(Json(json!({ "hub.challenge": q.get("hub.challenge") })))
}

#[derive(Deserialize)]
struct Event {
    object_type: String,
    aspect_type: String,
    object_id: u64,
}

async fn event(Json(e): Json<Event>) -> StatusCode {
    if e.object_type == "activity" && e.aspect_type == "create" {
        // Strava wants a 200 within 2s, so do the work in the background.
        tokio::spawn(async move {
            if let Err(err) = handle_activity(e.object_id).await {
                eprintln!("activity {}: {err}", e.object_id);
            }
        });
    }
    StatusCode::OK
}

async fn handle_activity(id: u64) -> reqwest::Result<()> {
    let http = reqwest::Client::new();
    let refresh_token = std::fs::read_to_string(REFRESH_TOKEN_FILE).expect("refresh_token file missing");
    let tokens: Value = http
        .post("https://www.strava.com/oauth/token")
        .form(&[
            ("client_id", env("STRAVA_CLIENT_ID")),
            ("client_secret", env("STRAVA_CLIENT_SECRET")),
            ("grant_type", "refresh_token".into()),
            ("refresh_token", refresh_token.trim().into()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if let Some(new) = tokens["refresh_token"].as_str() {
        std::fs::write(REFRESH_TOKEN_FILE, new).expect("can't save refresh_token");
    }
    let token = tokens["access_token"].as_str().unwrap_or_default();

    let activity: Value = http
        .get(format!("{API}/activities/{id}"))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if should_hide(&activity) {
        http.put(format!("{API}/activities/{id}"))
            .bearer_auth(token)
            .json(&json!({ "hide_from_home": true }))
            .send()
            .await?
            .error_for_status()?;
        eprintln!("activity {id}: hidden from feed");
    }
    Ok(())
}

fn should_hide(activity: &Value) -> bool {
    // `type` lumps Gravel/MTB rides under "Ride"; e-bike and virtual rides are separate types.
    activity["type"] == "Ride" && activity["distance"].as_f64().is_some_and(|m| m < 7000.0)
}

#[test]
fn hides_only_short_rides() {
    assert!(should_hide(&json!({ "type": "Ride", "distance": 6999.9 })));
    assert!(!should_hide(&json!({ "type": "Ride", "distance": 7000.0 })));
    assert!(!should_hide(&json!({ "type": "Run", "distance": 3000.0 })));
    assert!(!should_hide(&json!({ "type": "Ride" })));
}
