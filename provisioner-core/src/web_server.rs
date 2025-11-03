
use crate::traits::{ProvisioningBackend, UiAssetProvider};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use axum::body::Body;
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

    // For debug builds, listen on localhost for easy testing.
    // For release builds, listen on the captive portal IP.
    #[cfg(feature = "debug_build")]
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    #[cfg(not(feature = "debug_build"))]
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
    match state.frontend.get_asset(&path).await {
        Ok((data, mime)) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", mime)
            .body(Body::from(data)) // Explicitly convert to `axum::body::Body`
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from(format!("Asset not found: {}", path)))
            .unwrap(),
    }
}

/// API endpoint to scan for Wi-Fi networks.
async fn api_scan_wifi(State(state): WebServerState) -> impl IntoResponse {
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
    match state.backend.connect(&payload.ssid, &payload.password).await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "status": "success" }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
