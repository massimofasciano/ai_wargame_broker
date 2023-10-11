use axum::{
    routing::get,
    http::{StatusCode, Uri},
    response::{IntoResponse, Redirect},
    Json, Router,
    extract::{Path, State, Query, ConnectInfo, Host}};
use axum_server::tls_rustls::RustlsConfig;
use tokio::sync::Mutex;
use tower_http::{services::ServeDir, trace::{TraceLayer, self}};
use tracing::{info, debug, warn, error};
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
    statics: HashMap<String,ConfigStatic>,
    general: ConfigGeneral,
}

#[derive(Deserialize,Default,Debug,Clone)]
#[serde(default)]
struct ConfigGeneral {
    internal: Option<String>,
}

#[derive(Deserialize,Default,Debug,Clone)]
#[serde(default)]
struct ConfigStatic {
    uri: String,
    path: String,
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> (StatusCode, Json<GameReply>) {
    let mut reply = GameReply::default();
    let auth = params.auth.unwrap_or_default();
    if auth != state.client_auth {
        debug!("failed auth from {addr}");
        reply.success = false;
        reply.error = Some(String::from("invalid client auth"));
        return (StatusCode::UNAUTHORIZED, Json(reply));
    }
    let dict = state.game_data.lock().await;
    reply.data = dict.get(&gameid).map(Clone::clone);
    reply.success = true;
    if let Some(payload) = reply.data.as_ref() {
        debug!("game {} turn {:03} move {} -> {} read from {addr}",gameid,payload.turn,payload.from,payload.to);
    }
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
        debug!("failed auth from {addr}");
        reply.success = false;
        reply.error = Some(String::from("invalid client auth"));
        return (StatusCode::UNAUTHORIZED, Json(reply));
    }
    let mut dict = state.game_data.lock().await;
    info!("game {} turn {:03} move {} -> {} written from {addr}",gameid,payload.turn,payload.from,payload.to);
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
        error!("failed auth from {addr}");
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
        error!("failed auth from {addr}");
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
    debug!("{:#?}",config);

    let shared_state = Arc::new(SharedData { 
        client_auth: config.auth.client, 
        admin_auth: config.auth.admin,
        ..Default::default()
    });

    let mut app = Router::new()
        .route("/game/:gameid", get(game_get).post(game_post))
        .route("/admin/state", get(admin_state))
        .route("/admin/reset", get(admin_reset))
        .with_state(shared_state);

    for (_, static_dir) in config.statics {
        let trace_layer = TraceLayer::new_for_http()
            .make_span_with(trace::DefaultMakeSpan::new().level(tracing::Level::TRACE))
            .on_response(trace::DefaultOnResponse::new().level(tracing::Level::DEBUG));
        if static_dir.uri.ends_with('/') {
            app = app.nest_service(static_dir.uri.as_str(), ServeDir::new(static_dir.path))
                .layer(trace_layer);
        } else {
            // set up route for .../uri/ and redirect .../uri to .../uri/
            let with_slash = format!("{}/",static_dir.uri);
            app = app.nest_service(&with_slash, ServeDir::new(static_dir.path))
                .layer(trace_layer)
                .route(static_dir.uri.as_str(), get(|| async { 
                    let target = with_slash; // take ownership
                    Redirect::permanent(&target)
                }));
        }
    }

    #[cfg(feature = "internal")]
    {
        if let Some(internal_uri) = config.general.internal.as_deref() {
            if internal_uri.ends_with('/') {
                app = app.nest(internal_uri,internal::router())
            } else {
                // set up route for .../uri/ and redirect .../uri to .../uri/
                let with_slash = format!("{}/",internal_uri);
                app = app.nest(&with_slash,internal::router())
                    .route(internal_uri, get(|| async { 
                        let target = with_slash; // take ownership
                        Redirect::permanent(&target)
                    }));
            }
        }
    }

    let addr = SocketAddr::from(config.network);
    match config.tls.enabled {
        ConfigTLSType::Http => {
            warn!("listening on http://{addr}");
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
            warn!("listening on https://{addr}");
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
            warn!("listening on http+https://{addr}");
            axum_server_dual_protocol::bind_dual_protocol(addr, tls_config)
                .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                .await
                .unwrap();
        },
    }
}

#[cfg(feature = "internal")]
pub mod internal {
    use axum::{http::header, Router, routing::get};

    pub fn router() -> Router {
        Router::new()
            .route("/", get(|| async {
                ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], include_bytes!("../../ai_wargame_web/index.html"))
            }))
            .route("/game.js", get(|| async {
                ([(header::CONTENT_TYPE, "text/javascript")], include_bytes!("../../ai_wargame_web/game.js"))
            }))
            .route("/game.css", get(|| async {
                ([(header::CONTENT_TYPE, "text/css")], include_bytes!("../../ai_wargame_web/game.css"))
            }))
            .route("/pkg/ai_wargame_web.js", get(|| async {
                ([(header::CONTENT_TYPE, "text/javascript")], include_bytes!("../../ai_wargame_web/pkg/ai_wargame_web.js"))
            }))
            .route("/pkg/ai_wargame_web_bg.wasm", get(|| async {
                ([(header::CONTENT_TYPE, "application/wasm")], include_bytes!("../../ai_wargame_web/pkg/ai_wargame_web_bg.wasm"))
            }))
    }
}

