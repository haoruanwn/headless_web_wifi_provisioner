# simple-provisioner-wpadbus

一个最小化的、独立的 Rust 程序，专注于 `wpa_supplicant` D-Bus 交互和 AP 配网。

交叉编译
```bash
cross build \
   --target=armv7-unknown-linux-musleabihf \
   --release \
   --config 'target.armv7-unknown-linux-musleabihf.rustflags=["-C", "target-feature=+crt-static"]'
```

## 项目结构

```
simple-provisioner-wpadbus/
├── Cargo.toml                 # 项目依赖和配置
├── config/
│   └── wpa_dbus.toml         # AP 配置文件
├── ui/                        # 前端静态文件
│   ├── index.html
│   ├── app.js
│   ├── style.css
│   └── assets/
│       ├── logo.svg
│       └── wifi.svg
└── src/
    ├── main.rs               # 主入口
    ├── config.rs             # 配置加载
    ├── structs.rs            # 数据结构定义
    ├── backend.rs            # 核心后端（wpa_supplicant D-Bus）
    └── web_server.rs         # Web 服务器（Axum）
```

## 核心特性

### 1. 最小化设计
- **不依赖** `provisioner-core` 或 `provisioner-daemon`
- **无 trait**：所有方法都是 `WpaDbusBackend` 的直接实现
- **单一二进制**：`cargo build --release` 即可

### 2. 纯 wpa_supplicant D-Bus 实现
- 使用 `zbus` 库与 `wpa_supplicant` 通信
- 直接 Scan、AddNetwork、SelectNetwork 等 D-Bus 方法调用
- 信号监听：`ScanDone` 和 `PropertiesChanged`

### 3. TDM（时分复用）模式
- 启动时执行一次完整扫描
- 获取网络列表后启动 AP（hostapd + dnsmasq）
- 前端看到的始终是启动时的扫描结果（无需重新扫描）
- 连接成功后清理 AP，进入配置好的网络

### 4. Axum Web 服务器
- `/api/scan` - 返回缓存的网络列表
- `/api/connect` - 连接到指定网络
- `/api/backend_kind` - 返回 `{ "kind": "tdm" }`
- `/` 和 `/*` - 静态文件服务（使用 `tower-http`）

## 编译

```bash
cd simple-provisioner-wpadbus
cargo build --release
```

## 运行

```bash
# 需要 root 权限
sudo ./target/release/simple-provisioner-wpadbus

# 或启用日志
RUST_LOG=debug sudo ./target/release/simple-provisioner-wpadbus
```

## 前置要求

### 系统工具
- `wpa_supplicant`（带 D-Bus 支持）
- `hostapd`
- `dnsmasq`
- `ip`（iproute2）

### 配置文件
- `/etc/wpa_supplicant.conf`（用于 wpa_supplicant 启动）

### D-Bus 权限
- 需要 root 或相应的 polkit 权限以访问 `fi.w1.wpa_supplicant1` 服务

## 配置

编辑 `config/wpa_dbus.toml`：

```toml
ap_ssid = "Provisioner"          # AP 的 SSID
ap_psk = "12345678"              # AP 的密码
ap_gateway_cidr = "192.168.4.1/24"  # 网关和子网
ap_bind_addr = "192.168.4.1:80"  # Web 服务器绑定地址
```

## 工作流程

1. **启动** → 创建 `WpaDbusBackend` 实例
2. **扫描** → 通过 D-Bus 调用 `wpa_supplicant` 进行 Wi-Fi 扫描
3. **启动 AP** → 配置 IP、启动 hostapd 和 dnsmasq
4. **启动 Web 服务器** → 监听 192.168.4.1:80
5. **前端访问** → 获取网络列表、输入密码
6. **连接** → 通过 D-Bus 与目标网络连接
7. **清理** → 关闭 AP，恢复设置

## 开发与调试

### 查看日志
```bash
RUST_LOG=debug sudo ./target/release/simple-provisioner-wpadbus
RUST_LOG=trace sudo ./target/release/simple-provisioner-wpadbus  # 更详细
```

### 访问前端
在配置好的网络（192.168.4.x）中，打开浏览器访问：
```
http://192.168.4.1
```

### D-Bus 调试
```bash
# 查看 wpa_supplicant 对象
gdbus call --system --dest fi.w1.wpa_supplicant1 --object-path /fi/w1/wpa_supplicant1 --method fi.w1.wpa_supplicant1.GetInterface wlan0
```

## 下一步：抽象与重用

一旦这个 MVP 在硬件上跑通，你将获得宝贵的实战经验。此时可以：

1. 基于这个实现设计完美的 `trait`
2. 将核心逻辑提取到 `provisioner-core`
3. 支持多个后端（wpa_cli、nmcli 等）
4. 支持 Concurrent 模式（实时扫描）

## 许可证

MIT OR Apache-2.0
