## 使用说明

在构建的时候选择一个合适的前端和一个后端

参考以下场景：

1. 本地测试UI效果

   ```bash
   cargo run --features "provisioner-daemon/backend_mock, provisioner-daemon/ui_echo_mate"
   ```

2. 本地编译（systemd后端+echo-mate 主题）

   ```bash
   cargo run --features "provisioner-daemon/backend_systemd, provisioner-daemon/ui_echo_mate"
   
   cargo run --features "provisioner-daemon/backend_wpa_cli_exclusive, provisioner-daemon/ui_echo_mate"
   ```

3. 交叉编译（使用于buildroot的wpa_cli后端+ echo-mate 主题）

   ```bash
   cargo build --target=<target> --release --features "provisioner-daemon/backend_wpa_cli, provisioner-daemon/ui_echo_mate"
   ```


或者**用cross编译**

```bash
# 在 POSIX shell (Fedora, macOS) 中运行
cross build \
  --target=armv7-unknown-linux-musleabihf \
  --release \
  --features "provisioner-daemon/backend_wpa_dbus,provisioner-daemon/ui_bootstrap" \
  --config 'target.armv7-unknown-linux-musleabihf.rustflags=["-C", "target-feature=+crt-static"]'
```

