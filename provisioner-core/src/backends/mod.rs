// shared backend utilities (always available)
pub mod utils;

// 保留的后端（只导出三个）：
#[cfg(any(feature = "backend_mock_concurrent", feature = "backend_mock_TDM"))]
pub mod mock;

#[cfg(feature = "backend_wpa_cli_TDM")]
pub mod wpa_cli_TDM;

#[cfg(feature = "backend_nmcli_TDM")]
pub mod nmcli_TDM;

#[cfg(feature = "backend_nmdbus_TDM")]
pub mod nmdbus_tdm;

#[cfg(feature = "backend_wpa_dbus_TDM")]
pub mod wpa_dbus_tdm;
