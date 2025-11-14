# Soft AP Wi-Fi Provisioner

一个轻量级的 Wi-Fi 配网程序，通过启动一个临时的 Soft AP 和 Web 界面，来为嵌入式 Linux 设备配置 `wpa_supplicant`。

## 使用说明

### 交叉编译 

本项目依赖 `cross` 进行交叉编译。

```bash
# 例如，编译为 armv7 musleabihf 目标 (静态链接)
cross build \
   --target=armv7-unknown-linux-musleabihf \
   --release \
   --config 'target.armv7-unknown-linux-musleabihf.rustflags=["-C", "target-feature=+crt-static"]'
```
如果要开启音频播报功能，加上`--features "audio"`

### 运行调试 

直接运行编译好的二进制文件：

```bash
./provisioner
```

如果需要显示详细的调试日志：

```bash
RUST_LOG="debug,tower_http=debug" ./provisioner
```

## 设计原则与注意事项

本着 `Do one thing` 原则，本程序的核心职责**仅限于**：

1.  启动 Soft AP 热点。
2.  扫描 Wi-Fi。
3.  启动 WebServer。
4.  连接 Wi-Fi（通过 `wpa_supplicant`）。

本项目**不会**插手 Wi-Fi 自动连接、配网触发时机等应由操作系统或上层应用处理的事务。

默认配置下，所有 `wpa_supplicant` 配置文件均存放在 `/tmp` 目录，这意味着配网信息是临时的，设备重启后会丢失。

## 待实现清单 (Roadmap)

  * [ ] 为 Wi-Fi 自动连接（持久化）提供配置选项。
  * [ ] 添加可选的配网过程语音播报。
  * [ ] 减少对系统shell命令的依赖，不再依赖hostapd和dnsmsaq这两个系统工具

-----