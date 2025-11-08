
# 无头设备 Web 配网服务

## 简介

本项目旨在为需要频繁移动的无头 Linux 设备（比如网络摄像头、物联网设备、小型服务器等）提供一种基于 Web 界面的 Wi-Fi 配网服务。

项目设计了一个通用的抽象架构，分离了前端（UI）和后端（网络管理逻辑），并支持自定义行为策略。无论是使用 NetworkManager 进行网络管理的主流 Linux 发行版，还是使用 wpa_supplicant进行网络管理的嵌入式设备（如 Buildroot, Yocto），都可以通过替换后端实现，来灵活适配不同的前端界面。

### 核心策略

本项目为两种常见的硬件方案提供了接口，可以通过策略（Policy）和后端（Backend）的组合来实现：

1.  **分时复用 (TDM) 方案**
    * **适用场景：** 大部分的 Linux 设备（单无线网卡，且不支持 AP/STA 并发模式）。
    * **工作流程：** 在进入配网操作前，先在 STA 模式进行一次 Wi-Fi 扫描并缓存列表。然后切换到 AP 模式提供 Web 配网界面。
    * **权衡点：** 用户在 Web 界面上看到的 Wi-Fi 列表是缓存的，无法实时扫描，也无法实时反馈 Wi-Fi 链接是否成功，体验上有一定割裂。

2.  **并发 (Concurrent) 方案**
    * **适用场景：** 无线网卡支持 AP/STA 并发，或者拥有两个无线网卡的设备。
    * **工作流程：** 一个网卡（或虚拟接口）处于 AP 模式提供热点供 Web 组网，另一个网卡（或接口）进行实时的 Wi-Fi 扫描与链接。
    * **权衡点：** 硬件成本稍高，但用户体验（实时性）远好于 TDM 方案。

---

## 当前支持

### 后端支持

* `backend_networkmanager_TDM` 主要用于带有 NetworkManager 服务的桌面或服务器发行版。
* `backend_wpa_cl_TDM ` 依赖 `wpa_supplicant` 和 `wpa_cli` 工具，是嵌入式 Linux 环境的标准配置。
* `backend_mock` 仅用于模拟后端行为，方便在本地进行前端 UI 调试。
* （待补充...）

###  已添加的前端主题

* `ui_echo_mate`
* `ui_radxa_x4`
* (待补充...)

---

## 构建说明

在构建时，你需要根据目标平台，选择一个**后端 (backend)**、一个**前端 (ui)** 以及一个**策略 (policy)**。

### 1. 本地测试 UI 效果

```bash
# 本地快速测试：立即进入配网（On-Start 策略）
# 使用 mock 后端 和 echo_mate 主题
cargo run --release --features "\
   provisioner-daemon/backend_mock_TDM \
   provisioner-daemon/ui_echo_mate \
   provisioner-daemon/policy_on_start"

# 使用 mock 后端 和 radxa_x4 主题
cargo run --release --features "\
   provisioner-daemon/backend_mock \
   provisioner-daemon/ui_radxa_x4 \
   provisioner-daemon/policy_on_start"
```



### 2. 本地编译（目标为 Gnu/Linux 主机）

```bash
# 场景A：使用 networkmanager 后端（TDM）并立即进入配网
cargo build --release --features "\
   provisioner-daemon/backend_nmdbus_TDM \
   provisioner-daemon/ui_radxa_x4 \
   provisioner-daemon/policy_on_start"

# 场景B：使用 wpa_cli 后端（TDM）
cargo build --release --features "\
   provisioner-daemon/backend_wpa_cli_TDM \
   provisioner-daemon/ui_echo_mate"

# 场景C：使用 networkmanager 后端，并配置为“断线时进入配网”策略
cargo build --release --features "\
   provisioner-daemon/backend_networkmanager_TDM \
   provisioner-daemon/ui_echo_mate \
   provisioner-daemon/policy_daemon_if_disconnected"
```

### 3. 交叉编译（目标为嵌入式 Linux）

此场景通常使用 `wpa_cli` 后端。

#### 示例 1：使用 cargo 原生交叉编译

```bash
# 交叉编译 (目标平台为 <target>)
# 注意：同时选择 policy
cargo build --target=<target> --release --features "\
   provisioner-daemon/backend_wpa_cli \
   provisioner-daemon/ui_echo_mate \
   provisioner-daemon/policy_on_start"
```

#### 示例 2：使用 cross 工具编译 (推荐)

`cross` 工具能更好地处理 C 依赖和 musl 静态链接。

```bash
# 适用于 POSIX shell (Fedora, macOS, Linux)
# 目标：armv7 musl 静态链接
cross build \
   --target=armv7-unknown-linux-musleabihf \
   --release \
   --features "\
      provisioner-daemon/backend_wpa_cli_TDM \
      provisioner-daemon/ui_echo_mate \
      provisioner-daemon/policy_on_start" \
   --config 'target.armv7-unknown-linux-musleabihf.rustflags=["-C", "target-feature=+crt-static"]'
```

------

## 运行与调试

设置日志级别并以 `sudo` 运行（因为需要操作网络接口）：

```bash
# POSIX shell
sudo RUST_LOG="debug,tower_http=debug" ./target/release/provisioner-daemon
```

```bash
# 使用 rsync 部署到你的 Arch Linux (示例)
rsync -avz --delete target/release/provisioner-daemon archlinux:/home/hao/provisioner/
```