
// Conditionally compile and expose the disk-based frontend.
#[cfg(feature = "frontend_disk")]
pub mod disk_frontend;

// Conditionally compile and expose the embedded frontend.
#[cfg(feature = "frontend_embed")]
pub mod embed_frontend;
