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
        .route("/generate_204", get(handle_captive_portal))
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
/// 使用"发送并忘记"(Fire and Forget) 模式：
/// 立即返回 200 OK，然后在后台执行实际的连接工作。
/// 这避免了竞争条件：浏览器因为 AP 被关闭而无法接收响应。
async fn api_connect_tdm(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ConnectionRequest>,
) -> impl IntoResponse {
    tracing::debug!(ssid = %payload.ssid, "Handling /api/connect request (TDM)");

    // 克隆 backend Arc 以在后台任务中使用
    let backend_clone = state.backend.clone();

    // 生成后台任务来执行实际的连接工作
    tokio::spawn(async move {
        // connect 函数在后台运行，它包含：
        // 1. 停止 AP
        // 2. 连接到目标网络
        // 3. 运行 DHCP 获取 IP
        // 4. 调用 std::process::exit(0)
        if let Err(e) = backend_clone.connect(&payload).await {
            // 如果连接失败，connect 函数会自己重启 AP
            // 我们只需要记录错误并退出程序
            tracing::error!("Background connection task failed: {}", e);
            
            // 链接失败后自动退出程序（状态码 1 表示失败）
            println!("Connection failed. Shutting down application.");
            std::process::exit(1);
        }
    });

    // 立即返回 200 OK，在 AP 关闭之前发送给浏览器
    // 这样用户就能在手机上看到成功提示，即使设备随后断开 Wi-Fi
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "message": "Connection request received. Device is now switching networks."
        })),
    )
        .into_response()
}

/// 处理捕获门户检测请求（Captive Portal Detection）
/// 
/// 现代智能手机（Android、iOS）在连接到 Wi-Fi 后，会尝试访问已知的
/// 互联网检验 URL（如 connectivitycheck.gstatic.com/generate_204）来确认
/// 是否真的有互联网连接。
///
/// 我们的 dnsmasq 会劫持这个 DNS 请求并将其指向 192.168.4.1。
/// 这个处理器以静默方式响应它，避免不必要的日志警告。
async fn handle_captive_portal() -> impl IntoResponse {
    (StatusCode::NO_CONTENT, "")
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

