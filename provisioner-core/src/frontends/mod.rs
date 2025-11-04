// Conditionally compile the frontend providers based on the backend selection.
// This ensures that a provider is only compiled if its dependencies are available
// (e.g., `rust-embed` is only a dependency for real backends).

#[cfg(feature = "backend_mock")]
pub mod provider_disk;

// Only include the embedded frontend when we're not using the mock backend
// and when a UI theme feature (which provides embedded assets) is enabled.
#[cfg(all(not(feature = "backend_mock"), feature = "ui_echo_mate"))]
pub mod provider_embed;