## 使用说明

在构建的时候选择一个合适的前端和一个后端

参考以下场景：

1. 本地测试UI效果

    ```bash
    # 本地快速测试：立即进入配网（On-Start 策略）
    cargo run --features "\
       provisioner-daemon/backend_mock \
       provisioner-daemon/ui_echo_mate \
       provisioner-daemon/policy_on_start" 

    # 使用 radxa_x4 主题
    cargo run --features "\
       provisioner-daemon/backend_mock \
       provisioner-daemon/ui_radxa_x4 \
       provisioner-daemon/policy_on_start"
    ```

2. 本地编译（systemd后端+echo-mate 主题）

    ```bash
    # 使用 networkmanager 后端并立即进入配网
    cargo build --features "\
       provisioner-daemon/backend_networkmanager_TDM \
       provisioner-daemon/ui_radxa_x4 \
       provisioner-daemon/policy_on_start" --release

    # 编译 TDM 后端（示例）
    cargo build --features "\
       provisioner-daemon/backend_wpa_cli_TDM \
       provisioner-daemon/ui_echo_mate"

    cargo build --features "\
       provisioner-daemon/backend_networkmanager_TDM \
       provisioner-daemon/ui_echo_mate \
       provisioner-daemon/policy_daemon_if_disconnected"
    ```

3. 交叉编译（使用于buildroot的wpa_cli后端+ echo-mate 主题）

    ```bash
    # 交叉编译示例（注意：同时选择 policy）
    cargo build --target=<target> --release --features "\
       provisioner-daemon/backend_wpa_cli \
       provisioner-daemon/ui_echo_mate \
       provisioner-daemon/policy_on_start"
    ```


或者**用cross编译**

```bash
# 在 POSIX shell (Fedora, macOS) 中运行
cross build \
   --target=armv7-unknown-linux-musleabihf \
   --release \
    --features "\
      provisioner-daemon/backend_wpa_cli_TDM \
      provisioner-daemon/ui_echo_mate \
      provisioner-daemon/policy_daemon_if_disconnected" \
   --config 'target.armv7-unknown-linux-musleabihf.rustflags=["-C", "target-feature=+crt-static"]'
```

运行的时候查看日志：

```bash
export RUST_LOG="debug,tower_http=debug"
```

rsync -avz --delete target/release/ archlinux:/home/hao/provisioner/