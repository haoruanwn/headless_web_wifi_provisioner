# [cfg(feature = "frontend_disk")] // 只在启用此特性时编译
mod disk_impl {
    use crate::traits::UiAssetProvider;
    use async_trait::async_trait;
    use axum::response::IntoResponse;
    use tower_http::services::ServeFile;
    use std::path::PathBuf;
    use axum::http::{Request, StatusCode};
    use axum::body::Body;

    // 假设 UI 目录在项目根目录
    const UI_DIR: &str = "../ui";

    pub struct DiskFrontend;

    #[async_trait]
    impl UiAssetProvider for DiskFrontend {
        async fn get_asset(&self, path: &str) -> Result<impl IntoResponse, impl IntoResponse> {
            let path = if path.is_empty() { "index.html" } else { path };
            let file_path = PathBuf::from(UI_DIR).join(path);
            
            // 使用 tower-http 的 ServeFile 来安全地提供文件服务
            let request = Request::builder().body(Body::empty()).unwrap();
            match ServeFile::new(file_path).oneshot(request).await {
                Ok(response) => Ok(response.map(axum::body::boxed)),
                Err(_) => Err((StatusCode::NOT_FOUND, "Not Found")),
            }
        }
    }
}

// 条件化地导出
#[cfg(feature = "frontend_disk")]
pub use disk_impl::DiskFrontend;