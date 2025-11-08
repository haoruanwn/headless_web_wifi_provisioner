use crate::traits::{ConcurrentBackend, ConnectionRequest, TdmBackend, UiAssetProvider};
use axum::body::Body;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
// no local request structs; using traits::ConnectionRequest
// no direct use of SocketAddr here; backends provide bind addr via ApConfig
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

// --- App States with Type-Erased Trait Objects ---
// State for concurrent (real-time scanning) servers
struct ConcurrentAppState<F> {
    backend: Arc<dyn ConcurrentBackend + Send + Sync + 'static>,
    frontend: Arc<F>,
}

// State for TDM (Time-Division Multiplexing) servers
struct TdmAppState<F> {
    backend: Arc<dyn TdmBackend + Send + Sync + 'static>,
    frontend: Arc<F>,
    initial_networks: Arc<Mutex<Vec<crate::traits::Network>>>,
}

/// 启动实时扫描的 Web 服务器，用于支持并发能力的后端
pub fn start_concurrent_server<F>(
    backend: Arc<dyn ConcurrentBackend + Send + Sync + 'static>,
    frontend: Arc<F>,
) -> JoinHandle<Result<(), crate::Error>>
where
    F: UiAssetProvider + 'static,
{
    let app_state = Arc::new(ConcurrentAppState { backend, frontend });

    let app = Router::new()
        .route("/", get(serve_index_concurrent::<F>))
        .route("/api/backend_kind", get(api_backend_kind_concurrent))
        .route("/api/scan", get(api_scan_concurrent::<F>))
        .route("/api/connect", post(api_connect_concurrent::<F>))
        .route("/{*path}", get(serve_static_asset_concurrent::<F>))
        .with_state(app_state.clone());

    tokio::spawn(async move {
        app_state.backend.enter_provisioning_mode().await?;
        let cfg = app_state.backend.get_ap_config();
        println!("🌐 Concurrent Web server listening on {}", cfg.bind_addr);
        let listener = TcpListener::bind(cfg.bind_addr).await?;
        axum::serve(listener, app.into_make_service()).await?;
        Ok(())
    })
}

/// 启动 TDM（预扫描）的 Web 服务器，用于分时复用能力的后端
pub fn start_tdm_server<F>(
    backend: Arc<dyn TdmBackend + Send + Sync + 'static>,
    frontend: Arc<F>,
) -> JoinHandle<Result<(), crate::Error>>
where
    F: UiAssetProvider + 'static,
{
    tokio::spawn(async move {
        let networks = backend.enter_provisioning_mode_with_scan().await?;

        let app_state = Arc::new(TdmAppState {
            backend,
            frontend,
            initial_networks: Arc::new(Mutex::new(networks)),
        });

        let app = Router::new()
            .route("/", get(serve_index_tdm::<F>))
            .route("/api/backend_kind", get(api_backend_kind_tdm))
            .route("/api/scan", get(api_scan_tdm::<F>))
            .route("/api/connect", post(api_connect_tdm::<F>))
            .route("/{*path}", get(serve_static_asset_tdm::<F>))
            .with_state(app_state.clone());

        let cfg = app_state.backend.get_ap_config();
        println!("🌐 TDM Web server listening on {}", cfg.bind_addr);
        let listener = TcpListener::bind(cfg.bind_addr).await?;
        axum::serve(listener, app.into_make_service()).await?;
        Ok(())
    })
}

// --- Route Handlers (Generic & Concurrent) ---
async fn serve_index_concurrent<F>(
    State(state): State<Arc<ConcurrentAppState<F>>>,
) -> impl IntoResponse
where
    F: UiAssetProvider,
{
    serve_static_asset_concurrent(State(state), Path("index.html".to_string())).await
}

/// Serves static assets for concurrent backend.
async fn serve_static_asset_concurrent<F>(
    State(state): State<Arc<ConcurrentAppState<F>>>,
    Path(path): Path<String>,
) -> impl IntoResponse
where
    F: UiAssetProvider,
{
    tracing::trace!(asset_path = %path, "Attempting to serve static asset");
    match state.frontend.get_asset(&path).await {
        Ok((data, mime)) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", mime)
            .body(Body::from(data))
            .unwrap(),
        Err(_) => match state.frontend.get_asset("index.html").await {
            Ok((data, mime)) => Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", mime)
                .body(Body::from(data))
                .unwrap(),
            Err(e) => {
                tracing::error!(
                    asset_path = "index.html",
                    "FATAL: index.html not found: {}",
                    e
                );
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("FATAL: index.html not found"))
                    .unwrap()
            }
        },
    }
}

/// 对应前端请求，执行实时扫描
async fn api_scan_concurrent<F>(
    State(state): State<Arc<ConcurrentAppState<F>>>,
) -> impl IntoResponse
where
    F: UiAssetProvider,
{
    tracing::debug!("Handling /api/scan (Concurrent): performing real scan");
    match state.backend.scan().await {
        Ok(networks) => (StatusCode::OK, Json(networks)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// 返回后端类型：concurrent
async fn api_backend_kind_concurrent() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "kind": "concurrent" })),
    )
        .into_response()
}

// --- Route Handlers (Generic & TDM) ---

async fn serve_index_tdm<F>(State(state): State<Arc<TdmAppState<F>>>) -> impl IntoResponse
where
    F: UiAssetProvider,
{
    serve_static_asset_tdm(State(state), Path("index.html".to_string())).await
}

async fn serve_static_asset_tdm<F>(
    State(state): State<Arc<TdmAppState<F>>>,
    Path(path): Path<String>,
) -> impl IntoResponse
where
    F: UiAssetProvider,
{
    tracing::trace!(asset_path = %path, "Attempting to serve static asset (TDM)");
    match state.frontend.get_asset(&path).await {
        Ok((data, mime)) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", mime)
            .body(Body::from(data))
            .unwrap(),
        Err(_) => match state.frontend.get_asset("index.html").await {
            Ok((data, mime)) => Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", mime)
                .body(Body::from(data))
                .unwrap(),
            Err(e) => {
                tracing::error!(
                    asset_path = "index.html",
                    "FATAL: index.html not found: {}",
                    e
                );
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("FATAL: index.html not found"))
                    .unwrap()
            }
        },
    }
}

/// 对应前端请求，返回预扫描的网络列表
async fn api_scan_tdm<F>(State(state): State<Arc<TdmAppState<F>>>) -> impl IntoResponse
where
    F: UiAssetProvider,
{
    tracing::debug!("Handling /api/scan (TDM): returning cached list");
    let networks = state.initial_networks.lock().unwrap().clone();
    (StatusCode::OK, Json(networks)).into_response()
}

/// 返回后端类型：tdm
async fn api_backend_kind_tdm() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({ "kind": "tdm" }))).into_response()
}

async fn api_connect_concurrent<F>(
    State(state): State<Arc<ConcurrentAppState<F>>>,
    Json(payload): Json<ConnectionRequest>,
) -> impl IntoResponse
where
    F: UiAssetProvider,
{
    tracing::debug!(ssid = %payload.ssid, "Handling /api/connect request (Concurrent)");
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

async fn api_connect_tdm<F>(
    State(state): State<Arc<TdmAppState<F>>>,
    Json(payload): Json<ConnectionRequest>,
) -> impl IntoResponse
where
    F: UiAssetProvider,
{
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
