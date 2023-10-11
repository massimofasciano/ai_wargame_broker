use axum::{response::IntoResponse, http::header, Router, routing::get};

async fn index_html() -> impl IntoResponse {
    let headers = [(header::CONTENT_TYPE, "text/html; charset=utf-8")];
    (headers, include_bytes!("../../ai_wargame_web/index.html"))
}

async fn game_js() -> impl IntoResponse {
    let headers = [(header::CONTENT_TYPE, "text/javascript")];
    (headers, include_bytes!("../../ai_wargame_web/game.js"))
}

async fn game_css() -> impl IntoResponse {
    let headers = [(header::CONTENT_TYPE, "text/css")];
    (headers, include_bytes!("../../ai_wargame_web/game.css"))
}

async fn ai_wargame_js() -> impl IntoResponse {
    let headers = [(header::CONTENT_TYPE, "text/javascript")];
    (headers, include_bytes!("../../ai_wargame_web/pkg/ai_wargame_web.js"))
}

async fn ai_wargame_wasm() -> impl IntoResponse {
    let headers = [(header::CONTENT_TYPE, "application/wasm")];
    (headers, include_bytes!("../../ai_wargame_web/pkg/ai_wargame_web_bg.wasm"))
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(index_html))
        .route("/game.js", get(game_js))
        .route("/game.css", get(game_css))
        .route("/pkg/ai_wargame_web.js", get(ai_wargame_js))
        .route("/pkg/ai_wargame_web_bg.wasm", get(ai_wargame_wasm))
}