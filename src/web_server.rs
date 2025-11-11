use crate::backend::WpaCtrlBackend;
use crate::embed::EmbedFrontend;
use crate::structs::{ConnectionRequest, Network};
use crate::traits::UiAssetProvider;
use axum::{
    body::Body,
    extract::State,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

/// Web 服务器状态
struct AppState {
    backend: Arc<WpaCtrlBackend>,
    // TDM 模式：缓存启动时扫描的网络列表
    initial_networks: Arc<Mutex<Vec<Network>>>,
    // UI 资产提供器
    ui_provider: Arc<dyn UiAssetProvider>,
}

/// 启动 Web 服务器（TDM 模式）
pub async fn run_server(
    backend: Arc<WpaCtrlBackend>,
    initial_networks: Vec<Network>,
) -> anyhow::Result<()> {
    // 初始化 EmbedFrontend
    let ui_provider = Arc::new(EmbedFrontend::new());

    let app_state = Arc::new(AppState {
        backend: backend.clone(),
        initial_networks: Arc::new(Mutex::new(initial_networks)),
        ui_provider,
    });

    // 构建路由
    let app = Router::new()
        .route("/api/scan", get(api_scan_tdm))
        .route("/api/connect", post(api_connect_tdm))
        .route("/api/backend_kind", get(api_backend_kind_tdm))
        .fallback(get(serve_static_asset))
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

/// 处理静态资产的 Fallback 处理器
///
/// 这个处理器会捕获所有未被 API 路由匹配的 GET 请求，
/// 并尝试从 `UiAssetProvider` (即 EmbedFrontend) 中服务文件。
async fn serve_static_asset(
    State(state): State<Arc<AppState>>,
    uri: Uri,
) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();

    // 如果路径为空 (即请求根目录 "/"), 则服务 "index.html"
    if path.is_empty() {
        path = "index.html".to_string();
    }

    // 尝试从嵌入式资产中获取文件
    match state.ui_provider.get_asset(&path).await {
        Ok((data, mime)) => {
            // 成功：返回文件数据和正确的 Mime 类型
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                .body(Body::from(data))
                .unwrap_or_else(|_| {
                    (StatusCode::INTERNAL_SERVER_ERROR, "Failed to build response").into_response()
                })
        }
        Err(e) => {
            // 失败 (例如 404 Not Found)
            tracing::warn!("Failed to get asset: {} (Error: {})", path, e);
            // 对于 SPA (单页应用) 来说，
            // 更好的做法可能是在找不到文件时重定向回 index.html。
            // 但对于你这个简单的 UI，返回 404 是清晰且正确的。
            (StatusCode::NOT_FOUND, "Not Found").into_response()
        }
    }
}

