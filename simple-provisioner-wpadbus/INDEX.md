# 📚 simple-provisioner-wpadbus 文档索引

快速找到你需要的文档和代码。

---

## 🚀 快速开始（5分钟）

👉 **新手必读**：[QUICKSTART.md](./QUICKSTART.md)

- ⚡ 5 分钟快速启动
- 📋 系统要求清单
- 🔧 基本配置
- 🐛 常见问题排查
- 📡 API 测试示例

---

## 📖 完整文档

### 1. 项目概览和设计

📘 **[PROJECT_OVERVIEW.md](./PROJECT_OVERVIEW.md)** - 详细设计文档（600+ 行）

内容：
- 🏗️ 完整项目结构和模块分析
- 🔑 核心模块深度分解
- 📡 工作流程时序图
- 🛠️ 技术栈和依赖
- 🎯 设计决策说明
- 📊 代码行数统计
- 🧪 测试矩阵

**适合**：想深入理解系统架构的开发者

---

### 2. 使用说明

📘 **[README.md](./README.md)** - 项目基本说明

内容：
- 📋 项目结构
- 🎯 核心特性
- 🚀 编译和运行
- 🔧 配置参数
- 🌊 工作流程
- 💭 下一步计划

**适合**：想快速了解项目的新手

---

### 3. 完成总结

📘 **[COMPLETION_SUMMARY.md](./COMPLETION_SUMMARY.md)** - 项目完成情况

内容：
- ✅ 完成清单
- 📊 项目统计
- 🏗️ 架构概览
- 🔑 核心特性
- 📡 工作流程
- 📈 下一步计划

**适合**：快速了解项目完成度和质量评估

---

## 📝 源代码导航

### 核心实现

#### `src/backend.rs` ⭐⭐⭐ 最重要

这是项目的心脏，包含所有关键逻辑：

```rust
WpaDbusBackend              // 主结构体
├── ensure_conn()           // D-Bus 连接
├── ensure_iface_path()     // 获取接口
├── scan_internal()         // WiFi 扫描 ⭐
├── start_ap()              // 启动热点
├── stop_ap()               // 停止热点
├── setup_and_scan()        // 公开接口：扫描+AP
└── connect()               // 公开接口：连接网络
```

**代码行数**：~480 行  
**复杂度**：中等（D-Bus 信号处理）  
**学习价值**：⭐⭐⭐⭐⭐

---

#### `src/web_server.rs`

Axum Web 服务器实现：

```rust
AppState                     // 状态容器
├── backend                  // WpaDbusBackend 引用
└── initial_networks         // 缓存的网络列表

HTTP 路由
├── GET  /api/scan          // 返回缓存列表
├── POST /api/connect       // 处理连接请求
├── GET  /api/backend_kind  // 返回 backend 类型
└── GET  /*, /             // 静态文件服务
```

**代码行数**：~100 行  
**复杂度**：低（直接的请求处理）  
**学习价值**：⭐⭐⭐

---

#### `src/main.rs`

启动入口和初始化流程：

```rust
#[tokio::main]
async fn main() {
    1. 初始化日志
    2. 创建后端
    3. 执行扫描 + 启动 AP
    4. 启动 Web 服务器
}
```

**代码行数**：~40 行  
**复杂度**：低（清晰的流程）  
**学习价值**：⭐⭐

---

#### `src/config.rs` & `src/structs.rs`

辅助模块：

```rust
config.rs      // 从 TOML 加载配置
structs.rs     // 数据结构定义
```

**代码行数**：~55 行  
**复杂度**：很低  
**学习价值**：⭐

---

## 🎨 前端代码

### `ui/app.js` - 核心逻辑

```javascript
主要函数：
├── fetchWifiNetworks()     // 获取网络列表
├── renderList()            // 渲染网络列表
├── openModal()             // 显示密码输入框
├── connect()               // 执行连接
└── signalBarsHtml()        // 信号强度可视化
```

**行数**：~190 行  
**技术**：原生 JavaScript + Fetch API  
**学习价值**：⭐⭐⭐

---

### `ui/index.html` - 页面结构

标准 SPA（单页应用）结构：
- Header（标题 + 刷新按钮）
- Main（网络列表）
- Modal（密码输入框）
- Footer（版权信息）

---

### `ui/style.css` - 样式

Echo-mate 主题：
- 蓝色调（#0056d6）
- 现代设计（圆角、阴影）
- 响应式布局
- 平滑动画

---

## ⚙️ 配置文件

### `config/wpa_dbus.toml`

```toml
ap_ssid = "Provisioner"              # AP 名称
ap_psk = "12345678"                  # AP 密码
ap_gateway_cidr = "192.168.4.1/24"   # 网关 IP
ap_bind_addr = "192.168.4.1:80"      # Web 服务器监听
```

**修改方式**：编辑后重新 `cargo build`

---

### `Cargo.toml`

项目清单和依赖版本：

```toml
[dependencies]
tokio = "1"              # 异步运行时
axum = "0.8.6"           # Web 框架
zbus = "5.12.0"          # D-Bus 库
serde = "1"              # 序列化
...
```

---

## 🔍 按用途查找

### "我想学 D-Bus 编程"

👉 阅读：`src/backend.rs` 中的 `scan_internal()` 和 `connect()` 方法

关键片段：
1. `ensure_conn()` - 如何建立 D-Bus 连接
2. `root_proxy()` - 如何创建代理
3. 信号监听循环 - 如何处理异步信号
4. 属性访问 - 如何获取 D-Bus 属性

---

### "我想学 Rust 异步编程"

👉 阅读：`src/backend.rs` 中的信号监听和超时处理

关键代码：
```rust
let mut scan_done_stream = iface.receive_signal("ScanDone").await?;
match tokio::time::timeout(Duration::from_secs(15), async {
    // 等待信号
}).await?;
```

---

### "我想学 Web 服务开发"

👉 阅读：`src/web_server.rs` 和 `ui/app.js`

关键点：
- Axum 路由定义
- JSON 序列化/反序列化
- 状态管理
- 前端 API 调用

---

### "我想定制 UI"

👉 编辑：`ui/style.css` 和 `ui/app.js`

可定制项：
- 颜色主题（`:root` CSS 变量）
- 布局和响应式设计
- API 端点和逻辑
- 文本和文案

---

### "我想改变 AP 配置"

👉 编辑：`config/wpa_dbus.toml`

可配置项：
- AP SSID（网络名称）
- AP 密码
- IP 地址范围
- Web 服务器端口

**注意**：需要重新编译

---

## 📊 文件大小和复杂度

| 文件 | 行数 | 复杂度 | 学习价值 |
|------|------|--------|---------|
| backend.rs | 480 | 中 | ⭐⭐⭐⭐⭐ |
| web_server.rs | 100 | 低 | ⭐⭐⭐ |
| main.rs | 40 | 低 | ⭐⭐ |
| config.rs | 30 | 低 | ⭐ |
| structs.rs | 25 | 低 | ⭐ |
| app.js | 190 | 低 | ⭐⭐⭐ |
| style.css | 90 | 低 | ⭐⭐ |
| index.html | 50 | 低 | ⭐⭐ |

---

## 🎓 学习路径建议

### 初级（1-2 小时）
1. 读 QUICKSTART.md
2. 编译并运行程序
3. 在浏览器中测试
4. 查看日志输出

### 中级（2-4 小时）
1. 阅读 PROJECT_OVERVIEW.md
2. 研究 `backend.rs` 的结构
3. 理解 D-Bus 信号监听
4. 了解 Web 服务器架构

### 高级（4-8 小时）
1. 逐行阅读 `backend.rs`
2. 学习 D-Bus API 细节
3. 研究 `ui/app.js` 的前端逻辑
4. 实现自己的定制化功能

---

## 🔧 常见修改

### 改变 AP 名称和密码

编辑 `config/wpa_dbus.toml`：
```toml
ap_ssid = "MyNetworkName"
ap_psk = "MyPassword123"
```

### 改变 Web 服务器端口

编辑 `config/wpa_dbus.toml`：
```toml
ap_bind_addr = "192.168.4.1:8080"  # 改为 8080
```

### 改变 AP 的 IP 地址

编辑 `config/wpa_dbus.toml`：
```toml
ap_gateway_cidr = "192.168.5.1/24"  # 改为 192.168.5.x
ap_bind_addr = "192.168.5.1:80"
```

### 改变 UI 颜色主题

编辑 `ui/style.css` 中的 `:root` 部分：
```css
:root{
  --accent: #007bff;      /* 改为你的颜色 */
  --accent-2: #0056d6;
  --wifi-blue: #0d6efd;
}
```

---

## 📞 快速参考

### 日志级别

```bash
# 最少信息
sudo ./target/release/simple-provisioner-wpadbus

# 调试信息
RUST_LOG=debug sudo ./target/release/simple-provisioner-wpadbus

# 极详细
RUST_LOG=trace sudo ./target/release/simple-provisioner-wpadbus

# 只看特定模块
RUST_LOG=simple_provisioner_wpadbus=debug sudo ...
```

### 常用命令

```bash
# 检查编译
cargo check

# 构建调试版
cargo build

# 构建发布版
cargo build --release

# 运行测试
cargo test

# 查看文档
cargo doc --open
```

---

## 🎯 下一步

### 想添加功能？
- 阅读 PROJECT_OVERVIEW.md 了解架构
- 在 `src/backend.rs` 中添加新方法
- 在 `src/web_server.rs` 中添加新 API
- 在 `ui/app.js` 中添加前端逻辑

### 想优化性能？
- 查看 Cargo.toml 中的 release profile
- 分析热点代码（使用 profiler）
- 减少不必要的 D-Bus 调用

### 想支持多个后端？
- 阅读完成总结中的"下一步计划"
- 设计 trait 接口
- 实现不同的后端（wpa_cli、nmcli）
- 用工厂模式选择后端

---

## 📚 相关资源

### 官方文档
- [wpa_supplicant D-Bus 接口](https://w1.fi/wpa_supplicant/dbus/)
- [Axum 官方文档](https://docs.rs/axum/)
- [zbus 文档](https://docs.rs/zbus/)
- [Tokio 教程](https://tokio.rs/tokio/tutorial)

### 学习资源
- Rust 异步编程：https://tokio.rs
- D-Bus 详解：https://dbus.freedesktop.org/
- 现代 Web 开发：MDN Web Docs

---

## ✨ 总结

这个项目展示了如何用 Rust 从头到尾实现一个完整的系统：

✅ **后端**：D-Bus + 系统工具集成（~500 行）  
✅ **前端**：现代网页 UI（~300 行）  
✅ **文档**：详尽的设计文档和快速指南（~1000 行）  

**总规模**：2000 行代码 + 文档

**学习价值**：极高（涵盖系统编程、D-Bus、Web、前端等多个领域）

**生产就绪**：是（可立即部署）

---

**祝你探索愉快！** 🚀

有任何问题，详见相应文档。
