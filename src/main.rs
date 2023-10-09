use axum::{
    routing::get,
    // routing::post,
    http::StatusCode,
    response::IntoResponse,
    Json, Router,
    extract::{Path, State, Query}};
// use serde_json::json;
use tokio::sync::Mutex;
use std::{net::SocketAddr, sync::Arc, collections::HashMap, fs::read_to_string, str::FromStr};
use serde::{Deserialize, Serialize};

const CONFIG_FILE: &str = "ai_wargame_broker.json";

type SharedState = Arc<SharedData>;
type GameData = HashMap<String,GameTurn>;

#[derive(Default,Debug)]
struct SharedData {
    game_data: Mutex<GameData>,
    client_auth: String,
    admin_auth: String,
}

#[derive(Serialize,Default,Debug,Clone)]
struct GameReply {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    data: Option<GameTurn>,
}

#[derive(Serialize,Deserialize,Default,Debug,Clone,Copy)]
struct GameTurn {
    from : GameCoord,
    to : GameCoord,
    turn: u16,
}

#[derive(Serialize,Deserialize,Default,Debug,Clone,Copy)]
struct GameCoord {
    row: u8,
    col: u8,
}

#[derive(Serialize,Deserialize,Default,Debug,Clone)]
struct Config {
    client_auth: String,
    admin_auth: String,
    addr: String,
}

#[derive(Deserialize,Default,Debug,Clone)]
struct RequestParams {
    auth: Option<String>,
}

async fn game_get(
    Path(gameid): Path<String>,
    Query(params): Query<RequestParams>,
    State(state): State<SharedState>, 
) -> (StatusCode, Json<GameReply>) {
    let mut reply = GameReply::default();
    let auth = params.auth.unwrap_or_default();
    if auth != state.client_auth {
        reply.success = false;
        reply.error = Some(String::from("invalid client auth"));
        return (StatusCode::UNAUTHORIZED, Json(reply));
    }
    let dict = state.game_data.lock().await;
    reply.data = dict.get(&gameid).map(Clone::clone);
    reply.success = true;
    (StatusCode::OK, Json(reply))
}

async fn game_post(
    Path(gameid): Path<String>,
    Query(params): Query<RequestParams>,
    State(state): State<SharedState>, 
    Json(payload): Json<GameTurn>
) -> (StatusCode, Json<GameReply>) {
    let mut reply = GameReply::default();
    let auth = params.auth.unwrap_or_default();
    if auth != state.client_auth {
        reply.success = false;
        reply.error = Some(String::from("invalid client auth"));
        return (StatusCode::UNAUTHORIZED, Json(reply));
    }
    let mut dict = state.game_data.lock().await;
    dict.insert(gameid, payload);
    reply.data = Some(payload);
    reply.success = true;
    (StatusCode::OK, Json(reply))
}

async fn admin_state(
    Query(params): Query<RequestParams>,
    State(state): State<SharedState>, 
) -> impl IntoResponse {
    let auth = params.auth.unwrap_or_default();
    if auth != state.admin_auth {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let dict = state.game_data.lock().await;
    (StatusCode::OK, Json(Some(dict.clone()))).into_response()
}

async fn admin_reset(
    Query(params): Query<RequestParams>,
    State(state): State<SharedState>, 
) -> impl IntoResponse {
    let auth = params.auth.unwrap_or_default();
    if auth != state.admin_auth {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut dict = state.game_data.lock().await;
    dict.clear();
    (StatusCode::OK, Json(Some(dict.clone()))).into_response()
}

#[tokio::main]
async fn main() {
    let config: Config = serde_json::from_str(
        &read_to_string(CONFIG_FILE).expect("failed to open config file")
    ).expect("JSON was not well-formatted");

    let shared_state = Arc::new(SharedData { 
        client_auth: config.client_auth, 
        admin_auth: config.admin_auth,
        ..Default::default()
    });

    tracing_subscriber::fmt::init();
    let app = Router::new()
        .route("/game/:gameid", get(game_get).post(game_post))
        .route("/admin/state", get(admin_state))
        .route("/admin/reset", get(admin_reset))
        .with_state(shared_state);

    let addr = SocketAddr::from_str(&config.addr).expect("invalid address"); //   from(([0, 0, 0, 0], 8000));
    tracing::info!("listening on {addr}");
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
