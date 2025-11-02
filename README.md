# AstroBox-NG-Module-App-ESP32C3

针对嵌入式硬件esp32c3的AstroBox-NG客户端

## 编译
以下教程基于macOS Tahoe 26 (aarch64)

先安装：cmake、ninja、dfu-util、ldproxy (走cargo install)

必须使用esp-idf v5.3.3，推荐直接克隆idf仓库然后checkout到5.3.3分支，记得更新submodules，完事跑export.sh，然后直接cargo build / run即可

> 注意：该模块依赖独立的交叉编译工具链，已经从 `src-tauri` 顶层 Cargo workspace 中剥离。请直接在项目根目录执行 `cargo build --manifest-path src-tauri/modules/app_esp32c3/Cargo.toml` 或进入该目录后再运行 Cargo 命令。
