---

  1. 运行本地开发/调试模式 (模拟 UI)

  这个模式用于在开发电脑（如 macOS 或 Linux）上快速开发和测试 Web UI，它不需要真实的硬件，也不会尝试与系统服务（如 D-Bus）交互。

  用途：
   - 实时修改 ui/ 目录下的 HTML, CSS, JS 文件，刷新浏览器即可看到效果。
   - 使用模拟的 Wi-Fi 列表 (MockBackend) 测试前端逻辑。

  命令：
   1 cargo run --features provisioner-daemon/debug_build

  流程说明：
   1. 此命令会编译并运行 provisioner-daemon。
   2. --features provisioner-daemon/debug_build 标志会激活 debug_build 配置。
   3. 这会使程序在内部使用 MockBackend（模拟后端）和 DiskFrontend（从磁盘读取 UI 文件）。
   4. 服务启动后，可以在浏览器中打开 http://127.0.0.1:3000 来访问 Web 界面。

---

  2. 为目标设备构建生产/发布版本

  这个模式用于交叉编译，生成一个为目标嵌入式 Linux 设备优化的、自包含的单一可执行文件。

  用途：
   - 生成最终部署到设备上的程序。
   - 启用与 wpa_supplicant 交互的真实 D-Bus 后端 (DbusBackend)。
   - 将所有 UI 文件 (ui/ 目录) 嵌入到最终的二进制文件中，实现单一文件部署。

  命令：
   1 cargo build --target=<your-target-triple> --release --features provisioner-daemon/release_build

  流程说明：
   1. 这是一个 build 命令，它只编译，不运行。
   2. --target=<your-target-triple>: 这是最关键的部分。您需要将其中的 <your-target-triple> 替换为您 Buildroot 工具链的目标三元组。例如：
       - armv7-unknown-linux-gnueabihf (适用于 ARMv7 架构)
       - aarch64-unknown-linux-gnu (适用于 ARM64 架构)
   3. --release: 此标志会启用在 Cargo.toml 中定义的 [profile.release] 优化，例如 LTO、代码大小优化 (opt-level = "z") 等，以确保生成的二进制文件尽可能小。
   4. --features provisioner-daemon/release_build: 激活 release_build 配置，启用 DbusBackend 和 EmbedFrontend。

  最终产物：
   - 编译成功后，您将在以下路径找到最终的可执行文件：

    target/<your-target-triple>/release/provisioner-daemon

   - 您只需要将这一个 `provisioner-daemon` 文件复制到您的嵌入式设备上即可运行，它已经包含了所有需要的前端资源。



 **全新的使用流程**



 *1. 运行本地开发/调试模式*

 要运行**模拟后端**和**磁盘前端**（用于实时查看 UI 修改），请使用以下命令：



  1 cargo run --features "provisioner-daemon/backend_mock, provisioner-daemon/frontend_disk"

 服务启动后，访问 http://127.0.0.1:3000。您应该能看到包含信号和加密图标的详细 Wi-Fi 列表。



 *2. 为目标设备构建生产版本*

 要为您的嵌入式设备构建使用**真实 D-Bus 后端**和**嵌入式前端**的发布版本，请使用：



  1 cargo build --target=<your-target-triple> --release --features "provisioner-daemon/backend_dbus, provisioner-daemon/frontend_embed"

 （请记得替换 <your-target-triple>）



 *3. 自由组合（示例）*

 您现在可以自由组合。例如，如果您想在本地开发机上**测试真实的 D-Bus 后端**，同时**使用方便调试的磁盘前端**，您可以运行：



  1 # 注意：这需要在支持 D-Bus 且运行了 wpa_supplicant 的 Linux 环境下才能成功

  2 cargo run --features "provisioner-daemon/backend_dbus, provisioner-daemon/frontend_disk"

 这个命令会尝试连接到您电脑的 D-Bus 服务，同时从 ui/ 目录加载界面，让您可以在真实环境下调试 UI。



我们现在拥有一个更加健壮和可扩展的插件式架构。



 **全新的使用说明**



 *1. 核心选择*



 您现在可以从以下实现中进行选择：



  \- **后端 (`backend_\*`)**:

​    \- backend_wpa_dbus: 真实的 wpa_supplicant D-Bus 后端。

​    \- backend_systemd: (占位符) 用于 systemd-networkd 的后端。

​    \- backend_mock: 模拟后端，用于 UI 开发。

  \- **前端 (`frontend_\*`)**:

​    \- frontend_disk: 从磁盘加载 UI 文件，用于实时开发。

​    \- frontend_embed: 将 UI 文件嵌入到二进制文件中，用于发布。



 *2. 构建示例*



 **本地开发 (模拟后端 + 磁盘前端):**



  1 cargo run --features "provisioner-daemon/backend_mock, provisioner-daemon/frontend_disk"



 **为 Buildroot 设备构建 (真实 D-Bus 后端 + 嵌入式前端):**



  1 cargo build --target=<target> --release --features "provisioner-daemon/backend_wpa_dbus, provisioner-daemon/frontend_embed"



 **为 Systemd 设备构建 (Systemd 后端 + 嵌入式前端):**



  1 cargo build --target=<target> --release --features "provisioner-daemon/backend_systemd, provisioner-daemon/frontend_embed"

✦ 所有重构均已完成！您的架构思想已经完全实现。



 **最终使用说明**



 现在，您在构建时需要选择一个 backend_* 和一个 ui_*。交付方式（本地磁盘或嵌入式）会自动确定。



 **本地开发 (模拟后端 + Bootstrap 主题):**



  1 cargo run --features "provisioner-daemon/backend_mock, provisioner-daemon/ui_bootstrap"



 **本地开发 (模拟后端 + Simple 主题):**

  1 cargo run --features "provisioner-daemon/backend_mock, provisioner-daemon/ui_simple"



 **为 Buildroot 设备构建 (真实 D-Bus 后端 + Bootstrap 主题):**

  1 cargo build --target=<target> --release --features "provisioner-daemon/backend_wpa_dbus, provisioner-daemon/ui_bootstrap"



 这个架构更加清晰、健壮，且完全符合您的设想。