fn main() {
    println!("cargo:rerun-if-changed=../configs/nmcli_tdm.toml");
    println!("cargo:rerun-if-changed=../configs/nmdbus_tdm.toml");
    println!("cargo:rerun-if-changed=../configs/wpa_cli_tdm.toml");
    println!("cargo:rerun-if-changed=../configs/wpa_dbus_tdm.toml");
}