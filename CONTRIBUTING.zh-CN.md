# 参与开发

[English](CONTRIBUTING.md) · [简体中文](CONTRIBUTING.zh-CN.md)

欢迎帮助完善 FileManager。建议先阅读 [README](README.zh-CN.md)、[路线图](ROADMAP.zh-CN.md) 和[插件 SDK](docs/plugins.zh-CN.md)。可通过 [Issues](https://github.com/dBevil-z/FileManager/issues) 提交可复现问题或具体工作流建议。

## 开发

按照 README 安装 Linux 依赖，准备 Rust 1.91 或更新的工具链、Node.js 24 与 pnpm 10：

```bash
pnpm install --frozen-lockfile
pnpm desktop
```

## 检查

```bash
pnpm check
pnpm test
pnpm build
cargo fmt --all --check
cargo test -p filemanager-core
# 使用独立显示环境，避免受当前桌面剪贴板服务影响：
GTK_USE_PORTAL=0 NO_AT_BRIDGE=1 xvfb-run -a dbus-run-session -- cargo test -p filemanager
pnpm tauri build
```

原生剪贴板测试使用独立 selection，不覆盖用户剪贴板；文件测试使用临时数据和回收站。`FILEMANAGER_TEST_HOME`、`FILEMANAGER_DATA_DIR` 用于隔离开发场景，不要把真实文档当作破坏性测试夹具。

Linux workflow 在 Ubuntu 24.04 执行检查；手动运行和版本标签还会构建 deb/AppImage 产物。它不会自动创建 GitHub Release，单个 CI 环境也不等于所有桌面均兼容。

## 架构

- `crates/filemanager-core`：文件操作、任务、SQLite、撤销、预览和插件执行。
- `src-tauri`：GTK/WebKitGTK、剪贴板、托盘、全局快捷键与 KDE/D-Bus。
- `src`：React/TypeScript 界面和共享主题令牌。
- `plugins`：独立 JavaScript 工具与最小模板。

新增小工具先考虑插件。SDK 改动应同时更新 API 版本/兼容规则、模板、文档与能力边界测试。

## Pull request

说明触发条件、原先行为、修改结果和相关验证。桌面改动请注明发行版、桌面与 X11/Wayland 会话；界面改动附真实明暗截图和键盘/焦点检查。

文件操作保留冲突检测、取消和恢复；预览或插件改动检查缓存字节、后台进程生命周期及闲置释放。不要提交构建产物、日志、本地数据库、凭据和私人文档。

中文、英文贡献均可。行为或安装流程变化时，请同步两种语言的公开文档。

## 许可证状态

项目所有者尚未选择源码许可证。不要在没有明确决定时添加授权声明或替换第三方许可。
