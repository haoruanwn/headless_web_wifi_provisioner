use crate::backend::WpaCtrlBackend;
use crate::structs::{ConnectionRequest, Network};
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

/// Web 服务器状态
struct AppState {
    backend: Arc<WpaCtrlBackend>,
    // TDM 模式：缓存启动时扫描的网络列表
    initial_networks: Arc<Mutex<Vec<Network>>>,
}

/// 启动 Web 服务器（TDM 模式）
pub async fn run_server(
    backend: Arc<WpaCtrlBackend>,
    initial_networks: Vec<Network>,
) -> anyhow::Result<()> {
    let app_state = Arc::new(AppState {
        backend: backend.clone(),
        initial_networks: Arc::new(Mutex::new(initial_networks)),
    });

    // 构建路由
    let app = Router::new()
        .route("/api/scan", get(api_scan_tdm))
        .route("/api/connect", post(api_connect_tdm))
        .route("/api/backend_kind", get(api_backend_kind_tdm))
        .fallback_service(ServeDir::new("ui"))
        .with_state(app_state.clone());

    let bind_addr = backend.ap_config().bind_addr;
    tracing::info!("🌐 TDM Web server listening on {}", bind_addr);

    let listener = TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}

/// 返回缓存的扫描结果（TDM 模式）
async fn api_scan_tdm(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    tracing::debug!("Handling /api/scan (TDM): returning cached list");
    let networks = state.initial_networks.lock().unwrap().clone();
    (StatusCode::OK, Json(networks)).into_response()
}

/// 返回后端类型
async fn api_backend_kind_tdm() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({ "kind": "tdm" }))).into_response()
}

/// 处理连接请求（TDM 模式）
async fn api_connect_tdm(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ConnectionRequest>,
) -> impl IntoResponse {
    tracing::debug!(ssid = %payload.ssid, "Handling /api/connect request (TDM)");
    match state.backend.connect(&payload).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "success" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
