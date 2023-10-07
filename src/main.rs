use axum::{
    routing::get,
    // routing::post,
    http::StatusCode,
    // response::IntoResponse,
    Json, Router,
    extract::{Path, State}};
// use serde_json::json;
use tokio::sync::Mutex;
use std::{net::SocketAddr, sync::Arc, collections::HashMap};
use serde::{Deserialize, Serialize};

type SharedState = Arc<Mutex<HashMap<String,GameTurn>>>;

const GAMEID : &str = "qwerty";

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
    // if gameid != GAMEID {
    //     reply.success = false;
    //     reply.error = Some(String::from("bad game id"));
    //     return (StatusCode::NOT_FOUND, Json(reply));
    // }
    let dict = state.lock().await;
    reply.data = dict.get(&gameid).map(Clone::clone);
    reply.success = true;
    (StatusCode::OK, Json(reply))
}

async fn recv_move(
    Path(gameid): Path<String>, 
    State(state): State<SharedState>, 
    Json(payload): Json<GameTurn>
) -> (StatusCode, Json<GameReply>) {
    let mut reply = GameReply::default();
    // if gameid != GAMEID {
    //     reply.success = false;
    //     reply.error = Some(String::from("bad game id"));
    //     return (StatusCode::NOT_FOUND, Json(reply));
    // }
    let mut dict = state.lock().await;
    dict.insert(gameid, payload);
    reply.data = Some(payload);
    reply.success = true;
    (StatusCode::OK, Json(reply))
}

