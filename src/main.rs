use axum::{
    routing::get,
    // routing::post,
    http::StatusCode,
    // response::IntoResponse,
    Json, Router,
    extract::{Path, State, Query}};
// use serde_json::json;
use tokio::sync::Mutex;
use std::{net::SocketAddr, sync::Arc, collections::HashMap};
use serde::{Deserialize, Serialize};

type SharedData = HashMap<String,GameTurn>;
type SharedState = Arc<Mutex<SharedData>>;

const CLIENT_AUTH : &str = "s3cr3t";
const ADMIN_AUTH : &str = "ag3nt";

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

#[tokio::main]
async fn main() {
    let shared_state = Arc::new(Mutex::new(HashMap::new()));

    tracing_subscriber::fmt::init();
    let app = Router::new()
        .route("/game/:gameid", get(game_get).post(game_post))
        .route("/admin/state", get(admin_state))
        .route("/admin/reset", get(admin_reset))
        .with_state(shared_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    tracing::info!("listening on {addr}");
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn game_get(
    Path(gameid): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<SharedState>, 
) -> (StatusCode, Json<GameReply>) {
    let mut reply = GameReply::default();
    let auth = params.get("auth").map(AsRef::as_ref).unwrap_or("");
    if auth != CLIENT_AUTH {
        reply.success = false;
        reply.error = Some(String::from("invalid client auth"));
        return (StatusCode::NOT_FOUND, Json(reply));
    }
    let dict = state.lock().await;
    reply.data = dict.get(&gameid).map(Clone::clone);
    reply.success = true;
    (StatusCode::OK, Json(reply))
}

async fn game_post(
    Path(gameid): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<SharedState>, 
    Json(payload): Json<GameTurn>
) -> (StatusCode, Json<GameReply>) {
    let mut reply = GameReply::default();
    let auth = params.get("auth").map(AsRef::as_ref).unwrap_or("");
    if auth != CLIENT_AUTH {
        reply.success = false;
        reply.error = Some(String::from("invalid client auth"));
        return (StatusCode::NOT_FOUND, Json(reply));
    }
    let mut dict = state.lock().await;
    dict.insert(gameid, payload);
    reply.data = Some(payload);
    reply.success = true;
    (StatusCode::OK, Json(reply))
}

async fn admin_state(
    Query(params): Query<HashMap<String, String>>,
    State(state): State<SharedState>, 
) -> (StatusCode, Json<Option<SharedData>>) {
    let auth = params.get("auth").map(AsRef::as_ref).unwrap_or("");
    if auth != ADMIN_AUTH {
        return (StatusCode::NOT_FOUND, Json(None));
    }
    let dict = state.lock().await;
    (StatusCode::OK, Json(Some(dict.clone())))
}

async fn admin_reset(
    Query(params): Query<HashMap<String, String>>,
    State(state): State<SharedState>, 
) -> (StatusCode, Json<Option<SharedData>>) {
    let auth = params.get("auth").map(AsRef::as_ref).unwrap_or("");
    if auth != ADMIN_AUTH {
        return (StatusCode::NOT_FOUND, Json(None));
    }
    let mut dict = state.lock().await;
    dict.clear();
    (StatusCode::OK, Json(Some(dict.clone())))
}
