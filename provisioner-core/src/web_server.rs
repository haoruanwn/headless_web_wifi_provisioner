use crate::traits::{ConcurrentBackend, TdmBackend, UiAssetProvider};
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

/// Starts a concurrent (real-time scanning) server using a type-erased backend.
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
        .route("/api/scan", get(api_scan_concurrent::<F>))
        .route("/api/connect", post(api_connect_concurrent::<F>))
        .route("/{*path}", get(serve_static_asset_concurrent::<F>))
        .with_state(app_state.clone());

    tokio::spawn(async move {
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

/// Starts a TDM (pre-scanned) server using a type-erased backend.
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
            .route("/api/scan", get(api_scan_tdm::<F>))
            .route("/api/connect", post(api_connect_tdm::<F>))
            .route("/{*path}", get(serve_static_asset_tdm::<F>))
            .with_state(app_state.clone());

        let addr = SocketAddr::from(([192, 168, 4, 1], 80));
        println!("🌐 TDM Web server listening on {}", addr);
        let listener = TcpListener::bind(addr).await?;
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
                tracing::error!(asset_path = "index.html", "FATAL: index.html not found: {}", e);
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("FATAL: index.html not found"))
                    .unwrap()
            }
        },
    }
}

async fn api_scan_concurrent<F>(State(state): State<Arc<ConcurrentAppState<F>>>) -> impl IntoResponse
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
                tracing::error!(asset_path = "index.html", "FATAL: index.html not found: {}", e);
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("FATAL: index.html not found"))
                    .unwrap()
            }
        },
    }
}

async fn api_scan_tdm<F>(State(state): State<Arc<TdmAppState<F>>>) -> impl IntoResponse
where
    F: UiAssetProvider,
{
    tracing::debug!("Handling /api/scan (TDM): returning cached list");
    let networks = state.initial_networks.lock().unwrap().clone();
    (StatusCode::OK, Json(networks)).into_response()
}

// --- Common Route Handlers ---

#[derive(Deserialize)]
pub struct ConnectRequest {
    ssid: String,
    password: String,
}

async fn api_connect_concurrent<F>(
    State(state): State<Arc<ConcurrentAppState<F>>>,
    Json(payload): Json<ConnectRequest>,
) -> impl IntoResponse
where
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

async fn api_connect_tdm<F>(
    State(state): State<Arc<TdmAppState<F>>>,
    Json(payload): Json<ConnectRequest>,
) -> impl IntoResponse
where
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