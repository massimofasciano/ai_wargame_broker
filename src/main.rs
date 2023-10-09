use axum::{
    routing::get,
    http::{StatusCode, Uri},
    response::IntoResponse,
    Json, Router,
    extract::{Path, State, Query, ConnectInfo, Host}};
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

enum Error {
    RowToCharConversion
}

impl GameCoord {
    pub fn try_to_letter_number_string(self) -> Result<String,Error> {
        let row_char = if self.row < 26 { (self.row + b'A') as char } 
        else if self.row < 52 { (self.row - 26 + b'a') as char }
        else { '?' };
        if row_char != '?' { Ok(format!("{}{}", row_char, self.col)) } 
        else { Err(Error::RowToCharConversion) }
    }
    pub fn to_tuple_string(self) -> String {
        format!("({},{})", self.row, self.col)
    }
}

impl std::fmt::Display for GameCoord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.try_to_letter_number_string().unwrap_or(self.to_tuple_string()))
    }
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
    enabled: ConfigTLSType,
}

#[derive(Deserialize,Default,Debug,Clone,PartialEq)]
#[serde(rename_all = "lowercase")]
enum ConfigTLSType {
    #[default]
    Http,
    Https,
    Both,
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
    // ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> (StatusCode, Json<GameReply>) {
    let mut reply = GameReply::default();
    let auth = params.auth.unwrap_or_default();
    if auth != state.client_auth {
        // info!("failed auth from {addr}");
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<GameTurn>
) -> (StatusCode, Json<GameReply>) {
    let mut reply = GameReply::default();
    let auth = params.auth.unwrap_or_default();
    if auth != state.client_auth {
        // info!("failed auth from {addr}");
        reply.success = false;
        reply.error = Some(String::from("invalid client auth"));
        return (StatusCode::UNAUTHORIZED, Json(reply));
    }
    let mut dict = state.game_data.lock().await;
    info!("game {} turn {:03} move {} -> {} from {addr}",gameid,payload.turn,payload.from,payload.to);
    dict.insert(gameid, payload);
    reply.data = Some(payload);
    reply.success = true;
    (StatusCode::OK, Json(reply))
}

async fn admin_state(
    Query(params): Query<RequestParams>,
    State(state): State<SharedState>, 
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    uri: Uri, Host(hostname): Host,
) -> impl IntoResponse {
    info!("request from {addr} for {}{}",hostname,uri.path());
    let auth = params.auth.unwrap_or_default();
    if auth != state.admin_auth {
        info!("failed auth from {addr}");
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let dict = state.game_data.lock().await;
    (StatusCode::OK, Json(Some(dict.clone()))).into_response()
}

async fn admin_reset(
    Query(params): Query<RequestParams>,
    State(state): State<SharedState>, 
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    uri: Uri, Host(hostname): Host,
) -> impl IntoResponse {
    info!("request from {addr} for {}{}",hostname,uri.path());
    let auth = params.auth.unwrap_or_default();
    if auth != state.admin_auth {
        info!("failed auth from {addr}");
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
    match config.tls.enabled {
        ConfigTLSType::Http => {
            info!("listening on http://{addr}");
            axum::Server::bind(&addr)
                .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                .await
                .unwrap();
        },
        ConfigTLSType::Https => {
            let tls_config = RustlsConfig::from_pem_file(
                PathBuf::from(config.tls.cert),
                PathBuf::from(config.tls.key),
            ).await.unwrap();
            info!("listening on https://{addr}");
            axum_server::bind_rustls(addr, tls_config)
                .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                .await
                .unwrap();
        },
        ConfigTLSType::Both => {
            let tls_config = RustlsConfig::from_pem_file(
                PathBuf::from(config.tls.cert),
                PathBuf::from(config.tls.key),
            ).await.unwrap();
            info!("listening on http+https://{addr}");
            axum_server_dual_protocol::bind_dual_protocol(addr, tls_config)
                .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                .await
                .unwrap();
        },
    }
}
