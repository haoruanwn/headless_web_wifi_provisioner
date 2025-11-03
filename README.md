**最终使用说明**



 现在，您在构建时需要选择一个 backend_* 和一个 ui_*。交付方式（本地磁盘或嵌入式）会自动确定。



 **本地开发 (模拟后端 + Bootstrap 主题):**



  1 cargo run --features "provisioner-daemon/backend_mock, provisioner-daemon/ui_bootstrap"



 **本地开发 (模拟后端 + Simple 主题):**

  1 cargo run --features "provisioner-daemon/backend_mock, provisioner-daemon/ui_simple"



 **为 Buildroot 设备构建 (真实 D-Bus 后端 + Bootstrap 主题):**

  1 cargo build --target=<target> --release --features "provisioner-daemon/backend_wpa_dbus, provisioner-daemon/ui_bootstrap"



 这个架构更加清晰、健壮，且完全符合您的设想。


 ## 用cross编译
 cross编译

```bash
# 在 POSIX shell (Fedora, macOS) 中运行
cross build \
  --target=armv7-unknown-linux-musleabihf \
  --release \
  --features "provisioner-daemon/backend_wpa_dbus,provisioner-daemon/ui_bootstrap" \
  --config 'target.armv7-unknown-linux-musleabihf.rustflags=["-C", "target-feature=+crt-static"]'
```

