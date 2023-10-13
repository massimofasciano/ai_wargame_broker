use axum::{
    routing::{get, delete},
    http::{StatusCode, Uri, header, Request, HeaderValue},
    response::{IntoResponse, Redirect},
    Json, Router,
    extract::{Path, State, Query, ConnectInfo, Host}, TypedHeader, headers::{Authorization, authorization::Basic}, middleware::{Next, self}, debug_handler, Extension};
use axum_server::tls_rustls::RustlsConfig;
use tokio::{sync::Mutex, time::sleep};
use tower_http::{services::ServeDir, trace::{TraceLayer, self}};
use tracing::{info, debug, warn, error};
use std::{net::SocketAddr, sync::Arc, collections::HashMap, fs::read_to_string, str::FromStr, path::PathBuf, time::{Duration, SystemTime}};
use serde::{Deserialize, Serialize};
use askama::Template;
use nanoid::nanoid;

type SharedState = Arc<SharedData>;
type GameData = HashMap<String,GameTurn>;

#[derive(Default,Debug)]
struct SharedData {
    game_data: Mutex<GameData>,
    users: Vec<ConfigUser>,
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
    #[serde(skip_deserializing)]
    #[serde(skip_serializing)]
    updated: Option<SystemTime>,
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
    statics: Vec<ConfigStatic>,
    general: ConfigGeneral,
    users: Vec<ConfigUser>,
}

#[derive(Deserialize,Default,Debug,Clone)]
#[serde(default)]
struct ConfigGeneral {
    internal: Option<String>,
    expires: Option<u64>,
    cleanup: Option<u64>,
}

#[derive(Deserialize,Default,Debug,Clone)]
struct ConfigUser {
    name: String,
    #[serde(default)]
    role: ConfigUserRole,
    password: String,
}

#[derive(Deserialize,Default,Debug,Clone)]
#[serde(default)]
struct ConfigStatic {
    uri: String,
    path: String,
}

#[derive(Deserialize,Default,Debug,Clone)]
#[serde(default)]
struct ConfigTLS {
    cert: String,
    key: String,
    enabled: ConfigTLSType,
}

#[derive(Deserialize,Default,Debug,Copy,Clone,PartialEq)]
#[serde(rename_all = "lowercase")]
enum ConfigTLSType {
    #[default]
    Http,
    Https,
    Both,
}

#[derive(Deserialize,Default,Debug,Copy,Clone,PartialEq,PartialOrd)]
#[serde(rename_all = "lowercase")]
// the order of the roles is important for authentication (admin > user > guest)
enum ConfigUserRole {
    Guest,
    #[default]
    User,
    Admin,
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
    refresh: Option<usize>,
}

#[derive(Template)]
#[template(path = "hello.html")]
struct GameTemplate<'a> {
    refresh: Option<usize>,
    game_data: &'a GameData,
}

async fn game_generate(
    Query(_params): Query<RequestParams>,
    Extension(role): Extension<ConfigUserRole>,
    State(state): State<SharedState>, 
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    info!("{:?}",role);
    if role < ConfigUserRole::User {
        debug!("failed auth from {addr}");
        return authenticate().into_response();
    }
    let dict = state.game_data.lock().await;
    let mut gameid;
    loop {
        gameid = nanoid!(8);
        if dict.get(&gameid).is_none() { break; }
    }
    (StatusCode::OK, format!("{}\n",gameid)).into_response()
}

#[debug_handler]
async fn game_get(
    Path(gameid): Path<String>,
    Query(_params): Query<RequestParams>,
    Extension(role): Extension<ConfigUserRole>,
    State(state): State<SharedState>, 
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> (StatusCode, Json<GameReply>) {
    info!("{:?}",role);
    let mut reply = GameReply::default();
    if role < ConfigUserRole::User {
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
    Query(_params): Query<RequestParams>,
    Extension(role): Extension<ConfigUserRole>,
    State(state): State<SharedState>, 
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(mut payload): Json<GameTurn>
) -> (StatusCode, Json<GameReply>) {
    info!("{:?}",role);
    let mut reply = GameReply::default();
    if role < ConfigUserRole::User {
        debug!("failed auth from {addr}");
        reply.success = false;
        reply.error = Some(String::from("invalid client auth"));
        return (StatusCode::UNAUTHORIZED, Json(reply));
    }
    payload.updated = Some(SystemTime::now());
    let mut dict = state.game_data.lock().await;
    info!("game {} turn {:03} move {} -> {} written from {addr}",gameid,payload.turn,payload.from,payload.to);
    dict.insert(gameid, payload);
    reply.data = Some(payload);
    reply.success = true;
    (StatusCode::OK, Json(reply))
}

async fn admin_state(
    Query(params): Query<RequestParams>,
    Extension(role): Extension<ConfigUserRole>,
    State(state): State<SharedState>, 
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    uri: Uri, Host(hostname): Host,
) -> impl IntoResponse {
    info!("{:?}",role);
    debug!("request from {addr} for {}{}",hostname,uri.path());
    if role < ConfigUserRole::Admin {
        error!("failed auth from {addr}");
        return authenticate().into_response();
    }
    let dict = state.game_data.lock().await;
    (StatusCode::OK, GameTemplate { refresh: params.refresh, game_data: &dict }.into_response()).into_response()
}

async fn admin_clear(
    Query(_params): Query<RequestParams>,
    Extension(role): Extension<ConfigUserRole>,
    State(state): State<SharedState>, 
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    uri: Uri, Host(hostname): Host,
) -> impl IntoResponse {
    info!("{:?}",role);
    warn!("request from {addr} for {}{}",hostname,uri.path());
    if role < ConfigUserRole::Admin {
        error!("failed auth from {addr}");
        return authenticate().into_response();
    }
    let mut dict = state.game_data.lock().await;
    dict.clear();
    (StatusCode::OK, "cleared all games from internal state\n").into_response()
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

async fn cleaner(expires_secs: u64, cleanup_interval_secs: u64, state: SharedState) {
    loop {
        sleep(Duration::from_secs(cleanup_interval_secs)).await;
        debug!("cleaner starting");
        let mut dict = state.game_data.lock().await;
        dict.retain(|gameid, turndata| {
            if let Some(last_update) = turndata.updated {
                if let Ok(age) = last_update.elapsed() {
                    if age.as_secs() > expires_secs {
                        info!("game {gameid} has expired");
                        return false;
                    }
                }
            }
            true
        });
        debug!("cleaner ending");
    }
}

fn authenticate() -> impl IntoResponse {
    (
        [
            (header::WWW_AUTHENTICATE, HeaderValue::from_static("Basic realm=\"game broker\"")),
        ],
        StatusCode::UNAUTHORIZED
    )
}

async fn auth_basic<B>(
    auth: Option<TypedHeader<Authorization<Basic>>>,
    State(state): State<SharedState>, 
    mut request: Request<B>,
    next: Next<B>,
) -> impl IntoResponse {
    if let Some(auth) = auth {
        if let Some(user) = state.users.iter().find(|u| u.name == auth.username()) {
            info!("{:#?}",user);
            info!("{} {}",auth.username(),auth.password());
            if user.password == auth.password() {
                request.extensions_mut().insert(user.role);
                return next.run(request).await;
            }
        }
    }
    request.extensions_mut().insert(ConfigUserRole::Guest);
    next.run(request).await
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
        users: config.users,
        ..Default::default()
    });

    let mut app = Router::new()
        .route("/game", get(game_generate))
        .route("/game/:gameid", get(game_get).post(game_post))
        .route("/admin/state", get(admin_state))
        .route("/admin/clear", delete(admin_clear))
        .layer(middleware::from_fn_with_state(shared_state.clone(), auth_basic))
        .with_state(shared_state.clone());

    for static_dir in config.statics {
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
        macro_rules! get_bytes {
            ($ctype:expr,$s:expr) => {
                get(|| async {
                    ([(header::CONTENT_TYPE, $ctype)], include_bytes!(concat!("../../ai_wargame_web/",$s)))
                })
            };
        }
        let internal_router = Router::new()
            .route("/", get_bytes!("text/html; charset=utf-8","index.html"))
            .route("/game.js", get_bytes!("text/javascript","game.js"))
            .route("/game.css", get_bytes!("text/css","game.css"))
            .route("/pkg/ai_wargame_web.js", get_bytes!("text/javascript","pkg/ai_wargame_web.js"))
            .route("/pkg/ai_wargame_web_bg.wasm", get_bytes!("application/wasm","pkg/ai_wargame_web_bg.wasm"));
        if let Some(internal_uri) = config.general.internal.as_deref() {
            if internal_uri.ends_with('/') {
                app = app.nest(internal_uri,internal_router)
            } else {
                // set up route for .../uri/ and redirect .../uri to .../uri/
                let with_slash = format!("{}/",internal_uri);
                app = app.nest(&with_slash,internal_router)
                    .route(internal_uri, get(|| async { 
                        let target = with_slash; // take ownership
                        Redirect::permanent(&target)
                    }));
            }
        }
    }

    if let Some(interval_secs) = config.general.cleanup {
        if let Some(expires_secs) = config.general.expires {
            tokio::spawn(cleaner(expires_secs, interval_secs, shared_state.clone()));

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
