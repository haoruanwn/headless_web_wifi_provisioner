# [cfg(feature = "frontend_embed")] // 只在启用此特性时编译
mod embed_impl {
    use crate::traits::UiAssetProvider;
    use async_trait::async_trait;
    use axum::{
        http::{StatusCode, HeaderValue},
        response::{IntoResponse, Response},
    };
    use rust_embed::Embed;

    // 1. 编译时嵌入 'ui/' 目录 (路径相对于 Crate 根目录)
    // 注意：我们需要在 provisioner-core/Cargo.toml 中设置正确的 'ui' 路径
    // 或者在项目根目录的 Cargo.toml 中使用 [workspace.metadata.rust-embed]
    // 为了简单起见，我们假设 `ui` 目录在 `provisioner-core` 旁边
    #[derive(Embed)]
    #[folder = "../ui/"] 
    struct WebAssets;

    pub struct EmbeddedFrontend;

    #[async_trait]
    impl UiAssetProvider for EmbeddedFrontend {
        async fn get_asset(&self, path: &str) -> Result<impl IntoResponse, impl IntoResponse> {
            let path = if path.is_empty() { "index.html" } else { path };

            match WebAssets::get(path) {
                Some(file) => {
                    let mime = mime_guess::from_path(path).first_or_octet_stream();
                    let content_type = HeaderValue::from_str(mime.as_ref()).unwrap();
                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header(axum::http::header::CONTENT_TYPE, content_type)
                        .body(file.data.into())
                        .unwrap())
                }
                None => Err((StatusCode::NOT_FOUND, "Not Found")),
            }
        }
    }
}

// 条件化地导出实现
#[cfg(feature = "frontend_embed")]
pub use embed_impl::EmbeddedFrontend;