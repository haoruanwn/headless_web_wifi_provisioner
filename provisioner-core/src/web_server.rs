use crate::traits::{ConcurrentBackend, ProvisioningTerminator, TdmBackend, UiAssetProvider};
use axum::body::Body;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

// --- 并发服务器的状态 ---
struct ConcurrentAppState<B, F> {
    backend: Arc<B>,
    frontend: Arc<F>,
}

// --- TDM 服务器的状态 ---
struct TdmAppState<B, F> {
    backend: Arc<B>,
    frontend: Arc<F>,
    initial_networks: Arc<Mutex<Vec<crate::traits::Network>>>,
}

/// 启动并发（实时扫描）服务器
pub fn start_concurrent_server<B, F>(
    backend: Arc<B>,
    frontend: Arc<F>,
) -> JoinHandle<Result<(), crate::Error>>
where
    B: ConcurrentBackend + 'static,
    F: UiAssetProvider + 'static,
{
    let app_state = Arc::new(ConcurrentAppState { backend, frontend });

    let app = Router::new()
        .route("/", get(serve_index::<B, F>))
        .route("/api/scan", get(api_scan_concurrent::<B, F>))
        .route("/api/connect", post(api_connect_concurrent::<B, F>))
        .route("/{*path}", get(serve_static_asset_concurrent::<B, F>))
        .with_state(app_state.clone());

    tokio::spawn(async move {
        // 在 spawn 块中执行 enter_provisioning_mode
        app_state.backend.enter_provisioning_mode().await?;

        #[cfg(not(feature = "backend_mock"))]
        let addr = SocketAddr::from(([192, 168, 4, 1], 80));
        #[cfg(feature = "backend_mock")]
        let addr = SocketAddr::from(([0, 0, 0, 0], 3000));

        println!("🌐 Concurrent Web server listening on {}", addr);
        let listener = TcpListener::bind(addr).await?;
        axum::serve(listener, app.into_make_service()).await?;
        Ok(())
    })
}

/// 启动 TDM（先扫描）服务器
pub fn start_tdm_server<B, F>(
    backend: Arc<B>,
    frontend: Arc<F>,
) -> JoinHandle<Result<(), crate::Error>>
where
    B: TdmBackend + 'static,
    F: UiAssetProvider + 'static,
{
    tokio::spawn(async move {
        // TDM 后端在启动时获取一次扫描列表
        let networks = backend.enter_provisioning_mode_with_scan().await?;

        let app_state = Arc::new(TdmAppState {
            backend,
            frontend,
            initial_networks: Arc::new(Mutex::new(networks)),
        });

        let app = Router::new()
            .route("/", get(serve_index_tdm::<B, F>))
            .route("/api/scan", get(api_scan_tdm::<B, F>))
            .route("/api/connect", post(api_connect_tdm::<B, F>))
            .route("/{*path}", get(serve_static_asset_tdm::<B, F>))
            .with_state(app_state.clone());

        let addr = SocketAddr::from(([192, 168, 4, 1], 80));
        println!("🌐 TDM Web server listening on {}", addr);
        let listener = TcpListener::bind(addr).await?;
        axum::serve(listener, app.into_make_service()).await?;
        Ok(())
    })
}

// --- Route Handlers (Concurrent) ---

async fn serve_index<B, F>(
    State(_state): State<Arc<ConcurrentAppState<B, F>>>,
) -> impl IntoResponse
where
    B: ConcurrentBackend,
    F: UiAssetProvider,
{
    serve_static_asset_concurrent::<B, F>(State(_state), Path("index.html".to_string())).await
}

async fn serve_static_asset_concurrent<B, F>(
    State(state): State<Arc<ConcurrentAppState<B, F>>>,
    Path(path): Path<String>,
) -> impl IntoResponse
where
    B: ConcurrentBackend,
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
                tracing::error!(asset_path = "index.html", "FATAL: index.html not found: {}", e);
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("FATAL: index.html not found"))
                    .unwrap()
            }
        },
    }
}

async fn api_scan_concurrent<B, F>(
    State(state): State<Arc<ConcurrentAppState<B, F>>>,
) -> impl IntoResponse
where
    B: ConcurrentBackend,
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

// --- Route Handlers (TDM) ---

async fn serve_index_tdm<B, F>(
    State(_state): State<Arc<TdmAppState<B, F>>>,
) -> impl IntoResponse
where
    B: TdmBackend,
    F: UiAssetProvider,
{
    serve_static_asset_tdm::<B, F>(State(_state), Path("index.html".to_string())).await
}

async fn serve_static_asset_tdm<B, F>(
    State(state): State<Arc<TdmAppState<B, F>>>,
    Path(path): Path<String>,
) -> impl IntoResponse
where
    B: TdmBackend,
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
                tracing::error!(asset_path = "index.html", "FATAL: index.html not found: {}", e);
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("FATAL: index.html not found"))
                    .unwrap()
            }
        },
    }
}

async fn api_scan_tdm<B, F>(
    State(state): State<Arc<TdmAppState<B, F>>>,
) -> impl IntoResponse
where
    B: TdmBackend,
    F: UiAssetProvider,
{
    tracing::debug!("Handling /api/scan (TDM): returning cached list");
    let networks = state.initial_networks.lock().unwrap().clone();
    (StatusCode::OK, Json(networks)).into_response()
}

#[derive(Deserialize)]
pub struct ConnectRequest {
    ssid: String,
    password: String,
}

async fn api_connect_concurrent<B, F>(
    State(state): State<Arc<ConcurrentAppState<B, F>>>,
    Json(payload): Json<ConnectRequest>,
) -> impl IntoResponse
where
    B: ProvisioningTerminator,
    F: UiAssetProvider,
{
    tracing::debug!(ssid = %payload.ssid, "Handling /api/connect request (Concurrent)");
    match state.backend.connect(&payload.ssid, &payload.password).await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "status": "success" }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn api_connect_tdm<B, F>(
    State(state): State<Arc<TdmAppState<B, F>>>,
    Json(payload): Json<ConnectRequest>,
) -> impl IntoResponse
where
    B: ProvisioningTerminator,
    F: UiAssetProvider,
{
    tracing::debug!(ssid = %payload.ssid, "Handling /api/connect request (TDM)");
    match state.backend.connect(&payload.ssid, &payload.password).await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "status": "success" }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
