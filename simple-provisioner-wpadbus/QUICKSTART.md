# 🚀 快速启动指南 - simple-provisioner-wpadbus

## ⚡ 5 分钟快速上手

### 1. 构建项目

```bash
cd simple-provisioner-wpadbus
cargo build --release
```

**输出**：`target/release/simple-provisioner-wpadbus` (7.1 MB)

### 2. 配置系统

确保以下工具已安装：

```bash
# Ubuntu/Debian
sudo apt-get install -y wpa-supplicant hostapd dnsmasq iproute2

# 或检查是否已安装
which wpa_supplicant hostapd dnsmasq ip
```

### 3. 创建 wpa_supplicant 配置

```bash
sudo touch /etc/wpa_supplicant.conf
sudo chmod 600 /etc/wpa_supplicant.conf
```

**内容示例**（如果需要）：
```
ctrl_interface=/var/run/wpa_supplicant
update_config=1
```

### 4. 运行程序

```bash
# 简单运行（info 级别日志）
sudo ./target/release/simple-provisioner-wpadbus

# 调试模式（debug 级别）
RUST_LOG=debug sudo ./target/release/simple-provisioner-wpadbus

# 极详细（trace 级别）
RUST_LOG=trace sudo ./target/release/simple-provisioner-wpadbus
```

### 5. 连接设备并访问

从任何可以连接 WiFi 的设备：

1. 寻找 SSID: **"Provisioner"**
2. 密码: **"12345678"**
3. 打开浏览器访问: `http://192.168.4.1`
4. 选择你的 WiFi 网络，输入密码，点击连接
5. 等待设备连接到你的网络 ✅

---

## 📊 日志输出示例

### 成功启动的日志

```
🚀 Starting simple-provisioner-wpadbus...
📡 Executing initial D-Bus scan and starting AP...
ℹ️ wpa_supplicant D-Bus interface not available, attempting to start daemon...
ℹ️ wpa_supplicant daemon started, waiting for D-Bus interface...
✅ Initial scan complete, found 12 networks. AP started.
🌐 TDM Web server listening on 192.168.4.1:80
```

### 调试日志

```
DEBUG simple_provisioner_wpadbus: Handling /api/scan (TDM): returning cached list
DEBUG simple_provisioner_wpadbus: Handling /api/connect request (TDM)
DEBUG simple_provisioner_wpadbus: Processing connection to "MyWiFi"
DEBUG simple_provisioner_wpadbus: Connection state changed to "completed"
```

---

## 🔧 配置参数

编辑 `config/wpa_dbus.toml` 来自定义：

```toml
# AP 的网络名称
ap_ssid = "Provisioner"

# AP 的 WiFi 密码
ap_psk = "12345678"

# 网关 IP 和子网
ap_gateway_cidr = "192.168.4.1/24"

# Web 服务器监听地址
ap_bind_addr = "192.168.4.1:80"
```

**注意**：修改后需要重新 `cargo build`

---

## 🐛 常见问题

### Q1: 权限错误 "D-Bus connect failed"

```
Error: DBus connect failed: Message ...
```

**解决**：用 `sudo` 运行
```bash
sudo ./target/release/simple-provisioner-wpadbus
```

### Q2: wpa_supplicant 启动失败

```
Failed to spawn wpa_supplicant: ...
```

**解决**：
1. 检查 wpa_supplicant 是否安装：`which wpa_supplicant`
2. 检查配置文件：`ls -la /etc/wpa_supplicant.conf`
3. 手动启动测试：`sudo wpa_supplicant -B -iwlan0 -c/etc/wpa_supplicant.conf`

### Q3: 无法连接到 AP

**检查**：
1. WiFi 网卡是否支持 AP 模式：`iw list | grep -A 100 "AP$"`
2. hostapd/dnsmasq 是否运行：`ps aux | grep hostapd`
3. IP 配置：`ip addr show wlan0`

### Q4: 连接成功但无网络

这是正常的！当前实现：
- ✅ 连接到目标 WiFi
- ❌ 暂未配置 IP 获取（需要外部 DHCP 客户端或静态 IP）

**解决方案**：
```bash
# 在连接成功后运行
sudo dhclient wlan0
# 或配置静态 IP
sudo ip addr add 192.168.1.100/24 dev wlan0
sudo ip route add default via 192.168.1.1
```

---

## 🧪 测试 API

### 获取后端类型

```bash
curl http://192.168.4.1/api/backend_kind
```

**响应**：
```json
{"kind":"tdm"}
```

### 获取网络列表

```bash
curl http://192.168.4.1/api/scan
```

**响应**：
```json
[
  {
    "ssid": "MyWiFi",
    "signal": 75,
    "security": "WPA2"
  },
  {
    "ssid": "GuestNetwork",
    "signal": 45,
    "security": "Open"
  }
]
```

### 连接到网络

```bash
curl -X POST http://192.168.4.1/api/connect \
  -H "Content-Type: application/json" \
  -d '{"ssid":"MyWiFi","password":"12345"}'
```

**成功响应**：
```json
{"status":"success"}
```

**失败响应**：
```json
{"error":"Connection timed out"}
```

---

## 📈 性能指标

| 指标 | 值 |
|------|-----|
| 二进制大小 | 7.1 MB |
| 内存占用 | ~50 MB |
| 扫描时间 | 5-15 秒 |
| API 响应时间 | <100 ms |
| 连接超时 | 30 秒 |

---

## 🛑 停止程序

```bash
# Ctrl+C 优雅停止
^C

# 或在另一个终端运行
sudo killall simple-provisioner-wpadbus
```

**清理**：程序会自动清理：
- hostapd 进程
- dnsmasq 进程
- IP 地址配置
- 临时文件

---

## 📋 完整工作流程

```
1. cargo build --release
   ↓
2. sudo ./target/release/simple-provisioner-wpadbus
   ↓
3. [等待启动完成]
   ↓
4. 设备连接到 "Provisioner" WiFi
   ↓
5. 打开 http://192.168.4.1
   ↓
6. 选择目标 WiFi + 输入密码
   ↓
7. 点击连接
   ↓
8. 等待成功提示 ✅
   ↓
9. 设备进入配置网络
   ↓
10. 获取 IP 地址（DHCP 或静态）
```

---

## 🎓 下一步学习

1. **源代码**：阅读 `src/backend.rs` 理解 D-Bus 交互
2. **前端**：修改 `ui/app.js` 定制用户界面
3. **配置**：调整 `config/wpa_dbus.toml` 的参数
4. **日志**：使用 `RUST_LOG=trace` 查看详细调试信息

---

## 💬 支持和反馈

遇到问题？

1. 检查日志输出：`RUST_LOG=debug sudo ...`
2. 查看 `PROJECT_OVERVIEW.md` 获取详细设计文档
3. 阅读 `README.md` 了解项目结构

---

**祝你使用愉快！** 🎉

记住：这是一个 MVP（最小可行产品），目标是验证概念和积累实战经验。后续可以扩展功能、支持更多模式、集成到更大系统中。
