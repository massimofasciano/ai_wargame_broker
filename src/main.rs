use axum::{
    routing::get,
    http::StatusCode,
    response::IntoResponse,
    Json, Router,
    extract::{Path, State, Query}};
use axum_server::tls_rustls::RustlsConfig;
use tokio::sync::Mutex;
use tracing::info;
use std::{net::SocketAddr, sync::Arc, collections::HashMap, fs::read_to_string, str::FromStr, path::PathBuf};
use serde::{Deserialize, Serialize};

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

#[derive(Deserialize,Default,Debug,Clone)]
#[serde(default)]
struct Config {
    network: ConfigNetwork,
    tls: ConfigTLS,
    auth: ConfigAuth,
}

#[derive(Deserialize,Default,Debug,Clone)]
#[serde(default)]
struct ConfigAuth {
    client: String,
    admin: String,
}

#[derive(Deserialize,Default,Debug,Clone)]
#[serde(default)]
struct ConfigTLS {
    cert: String,
    key: String,
    enabled: bool,
}

#[derive(Deserialize,Debug,Clone)]
#[serde(default)] 
struct ConfigNetwork {
    ip: String,
    port: u32,
}

impl Default for ConfigNetwork {
    fn default() -> Self {
        ConfigNetwork { 
            ip: "127.0.0.1".to_string(), 
            port: 8000,
        }
    }
}

impl From<ConfigNetwork> for SocketAddr {
    fn from(value: ConfigNetwork) -> Self {
        SocketAddr::from_str(&format!("{}:{}",value.ip,value.port)).expect("invalid address")
    }
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

fn get_config_file_name(in_cwd: bool) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|pb| if in_cwd {
            pb.file_name().map(PathBuf::from)
        } else {
            Some(pb)
        })
        .unwrap_or(PathBuf::from(env!("CARGO_PKG_NAME")))
        .with_extension("toml")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("Loading config from {:?} or {:?}",get_config_file_name(true),get_config_file_name(false));

    let config: Config = toml::from_str(
        &read_to_string(get_config_file_name(true))
            .or(read_to_string(get_config_file_name(false)))
            .unwrap_or(String::from(""))
    ).expect("TOML was not well-formatted");

    info!("{:#?}",config);

    let shared_state = Arc::new(SharedData { 
        client_auth: config.auth.client, 
        admin_auth: config.auth.admin,
        ..Default::default()
    });

    let app = Router::new()
        .route("/game/:gameid", get(game_get).post(game_post))
        .route("/admin/state", get(admin_state))
        .route("/admin/reset", get(admin_reset))
        .with_state(shared_state);

    let addr = SocketAddr::from(config.network);
    if config.tls.enabled {
        let tls_config = RustlsConfig::from_pem_file(
            PathBuf::from(config.tls.cert),
            PathBuf::from(config.tls.key),
        ).await.unwrap();
        info!("listening on https://{addr}");
        axum_server::bind_rustls(addr, tls_config)
            .serve(app.into_make_service())
            .await
            .unwrap();
    
    } else {
        info!("listening on http://{addr}");
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
            .unwrap();
    }
}
