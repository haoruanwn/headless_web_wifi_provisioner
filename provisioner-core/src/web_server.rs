use crate::traits::{ProvisioningBackend, UiAssetProvider};
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
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

// The shared state for our web server.
// By using `Arc<dyn Trait>`, we can inject any implementation
// that satisfies the trait bounds.
pub type WebServerState = State<Arc<AppState>>;

pub struct AppState {
    pub backend: Arc<dyn ProvisioningBackend>,
    pub frontend: Arc<dyn UiAssetProvider>,
}

/// Starts the Axum web server.
///
/// # Arguments
/// * `backend` - An `Arc` wrapping a `ProvisioningBackend` implementation.
/// * `frontend` - An `Arc` wrapping a `UiAssetProvider` implementation.
///
/// # Returns
/// A `JoinHandle` for the server task.
pub fn start_web_server(
    backend: Arc<dyn ProvisioningBackend>,
    frontend: Arc<dyn UiAssetProvider>,
) -> JoinHandle<Result<(), crate::Error>> {
    let app_state = Arc::new(AppState { backend, frontend });

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/api/scan", get(api_scan_wifi))
        .route("/api/connect", post(api_connect_wifi))
        .route("/{*path}", get(serve_static_asset))
        .with_state(app_state);

    // The listening address is now determined by the backend choice.
    // Mock backend implies local development.
    #[cfg(feature = "backend_mock")]
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    // Real backends imply deployment on the device.
    #[cfg(not(feature = "backend_mock"))]
    let addr = SocketAddr::from(([192, 168, 4, 1], 80));

    println!("🌐 Web server listening on {}", addr);

    tokio::spawn(async move {
        let listener = TcpListener::bind(addr).await?;
        axum::serve(listener, app.into_make_service())
            .await
            .map_err(|e| crate::Error::WebServer(e.into()))
    })
}

// --- Route Handlers ---

/// Serves the main `index.html` file.
async fn serve_index(State(state): WebServerState) -> impl IntoResponse {
    serve_static_asset(State(state), Path("index.html".to_string())).await
}

/// Serves a static asset (e.g., CSS, JS) from the frontend provider.
async fn serve_static_asset(
    State(state): WebServerState,
    Path(path): Path<String>,
) -> impl IntoResponse {
    // 添加日志
    tracing::trace!(asset_path = %path, "Attempting to serve static asset");
    match state.frontend.get_asset(&path).await {
        Ok((data, mime)) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", mime)
            .body(Body::from(data)) // Explicitly convert to `axum::body::Body`
            .unwrap(),
        Err(e) => {
            // 【添加】当文件未找到时，打印一条警告日志
            tracing::warn!(asset_path = %path, "Asset not found: {}", e);
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from(format!("Asset not found: {}", path)))
                .unwrap()
        }
    }
}

/// API endpoint to scan for Wi-Fi networks.
async fn api_scan_wifi(State(state): WebServerState) -> impl IntoResponse {
    tracing::debug!("Handling /api/scan request");
    match state.backend.scan().await {
        Ok(networks) => (StatusCode::OK, Json(networks)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct ConnectRequest {
    ssid: String,
    password: String,
}

/// API endpoint to connect to a Wi-Fi network.
async fn api_connect_wifi(
    State(state): WebServerState,
    Json(payload): Json<ConnectRequest>,
) -> impl IntoResponse {
    tracing::debug!(ssid = %payload.ssid, "Handling /api/connect request");
    match state
        .backend
        .connect(&payload.ssid, &payload.password)
        .await
    {
        Ok(_) => {
            tracing::info!("Wi-Fi connection successful, exiting provisioning mode.");
            // On successful connection, tear down the AP
            if let Err(e) = state.backend.exit_provisioning_mode().await {
                tracing::error!("Error exiting provisioning mode: {}", e);
                // Fall through to return success to the user anyway, as Wi-Fi is connected.
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({ "status": "success" })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::warn!("Connection failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}
