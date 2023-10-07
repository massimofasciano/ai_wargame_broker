use axum::{
    routing::get,
    // routing::post,
    http::StatusCode,
    // response::IntoResponse,
    Json, Router,
    extract::{Path, State}};
// use serde_json::json;
use tokio::sync::Mutex;

use std::{net::SocketAddr, sync::Arc};
use serde::{Deserialize, Serialize};

type SharedState = Arc<Mutex<GameTurn>>;

const GAMEID : &str = "qwerty";

#[derive(Serialize,Deserialize,Default,Debug,Clone)]
struct GameReply {
    success: bool,
    error: String,
    data: GameTurn,
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
    let shared_state = Arc::new(Mutex::new(GameTurn::default()));

    tracing_subscriber::fmt::init();
    let app = Router::new()
        .route("/:gameid", get(send_move).post(recv_move))
        .with_state(shared_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    tracing::info!("listening on http://{addr}/{GAMEID}");
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn send_move(
    Path(gameid): Path<String>,
    State(state): State<SharedState>, 
) -> (StatusCode, Json<GameReply>) {
    let mut reply = GameReply::default();
    if gameid != GAMEID {
        reply.success = false;
        reply.error = String::from("bad game id");
        return (StatusCode::NOT_FOUND, Json(reply));
    }
    reply.success = true;
    reply.data = *(state.lock().await);
    (StatusCode::OK, Json(reply))
}

async fn recv_move(
    Path(gameid): Path<String>, 
    State(state): State<SharedState>, 
    Json(payload): Json<GameTurn>
) -> (StatusCode, Json<GameReply>) {
    let mut reply = GameReply::default();
    if gameid != GAMEID {
        reply.success = false;
        reply.error = String::from("bad game id");
        return (StatusCode::NOT_FOUND, Json(reply));
    }
    reply.data = payload;
    reply.success = true;
    *(state.lock().await) = payload;
    (StatusCode::OK, Json(reply))
}

