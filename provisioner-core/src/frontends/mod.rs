// Conditionally compile the frontend providers based on the backend selection.
// This ensures that a provider is only compiled if its dependencies are available
// (e.g., `rust-embed` is only a dependency for real backends).

#[cfg(feature = "backend_mock")]
pub mod provider_disk;

#[cfg(not(feature = "backend_mock"))]
pub mod provider_embed;