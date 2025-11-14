## 简介


## 使用说明


### 交叉编译
需要安装rust工具链和cross
```bash
cross build \
   --target=armv7-unknown-linux-musleabihf \
   --release \
   --config 'target.armv7-unknown-linux-musleabihf.rustflags=["-C", "target-feature=+crt-static"]'
```

### 运行调试
直接运行
```bash
./provisioner
```
如果需要显示详细日志
```bash
RUST_LOG="debug,tower_http=debug" ./provisioner
```

## 注意事项
本着`Do one thing`原则，本程序只负责启动ap、扫描Wi-Fi、启动webserver、链接Wi-Fi的操作，至于Wi-Fi启动后自动连接，以及什么时候进行配网操作，这是操作系统配置需要操心的时，本项目不会插手这些。

默认的配置下，wpa_supplicant的配置文件会存放到`/tmp`目录，也就是说所有配置都是临时的，重启后会直接消失，不会记忆Wi-Fi，如果需要记忆Wi-Fi链接，或者开机后自动连接之前配置好的Wi-Fi，可以把wpa_suppicant配置文件的位置修改到`/etc`路径下，并且基于它设置开机自启的任务
```toml
# 自包含的配置文件路径 
# （使用 /tmp 或 /run 目录，避免依赖 /etc 系统配置）
hostapd_conf_path = "/tmp/provisioner_hostapd.conf"
wpa_conf_path = "/tmp/provisioner_wpa.conf"

# 修改为
wpa_conf_path = "/etc/provisioner_wpa.conf"
```