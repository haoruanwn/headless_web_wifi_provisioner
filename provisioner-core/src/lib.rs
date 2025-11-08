//! Core library for the headless Wi-Fi provisioner.
//! This crate defines the core traits (interfaces) and data structures,
//! and provides different implementations for backends (Wi-Fi control)
//! and frontends (UI asset delivery) controlled by feature flags.

pub mod backends;
pub mod config;
pub mod factory;
pub mod frontends;
pub mod traits;
pub mod web_server; // expose config parsing utilities

// Define a shared Error and Result type for the entire crate.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Command failed: {0}")]
    CommandFailed(String),

    // WPA D-Bus backend related errors removed when backend_wpa_dbus feature was dropped.
    #[error("Web server error: {0}")]
    WebServer(#[from] axum::BoxError),

    #[error("Asset not found: {0}")]
    AssetNotFound(String),

    #[error("UTF-8 conversion error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    // Add other specific error types here as needed.
    // For example, when we add the D-Bus backend:
    //
    // #[cfg(feature = "backend_dbus")]
    // #[error("D-Bus error: {0}")]
    // Dbus(#[from] zbus::Error),
}

/// A specialized `Result` type for this crate's operations.
pub type Result<T> = std::result::Result<T, Error>;
