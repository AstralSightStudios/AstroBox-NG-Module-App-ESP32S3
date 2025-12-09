# AstroBox-NG-Module-App-ESP32C3

针对嵌入式硬件esp32c3的AstroBox-NG客户端

## 编译
以下教程基于macOS Tahoe 26 (aarch64)

先安装：cmake、ninja、dfu-util、ldproxy (走cargo install)

必须使用esp-idf v5.3.3，执行编译时会自动下载，但推荐你先自己装好idf v5.3.3，然后使用zed编辑器并install cli，然后在终端中先执行idf的export.sh，接着直接zed -n <folder path>打开项目以节省时间和生命

> 注意：该模块依赖独立的交叉编译工具链，已经从 `src-tauri` 顶层 Cargo workspace 中剥离。请直接进入该目录后再运行 Cargo 命令。为了让rust-analyzer正常工作，你通常也需要在编辑器中单独打开该模块的文件夹。

此库使用 AGPL 3.0 授权

This library is licensed under AGPL 3.0

## 额外条款 / Additional Terms
根据AGPL 3.0所述可选附加条款，本项目额外附加署名要求，使用此项目需在遵守AGPL 3.0条款后额外为此项目添加署名，署名包括但不限于本项目仓库地址，作者名等。

注: 附加条款以中文版为准，其他语言仅供参考！

According to the optional additional terms stated in AGPL 3.0, this project includes an additional attribution requirement. When using this project, after complying with the terms of AGPL 3.0, you must also add attribution for this project, which includes but is not limited to the project repository address, the author's name, etc.

Note: The additional terms are based on the Chinese version. Other languages are for reference only!