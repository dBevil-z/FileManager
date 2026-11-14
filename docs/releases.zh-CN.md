# 更新与发布

[English](releases.md) · [简体中文](releases.zh-CN.md)

官方仓库：[dBevil-z/FileManager](https://github.com/dBevil-z/FileManager)。安装包与版本说明见 [GitHub Releases](https://github.com/dBevil-z/FileManager/releases/latest)。

## 在应用中更新

从 **设置 → 关于与更新 → 软件更新** 或 **关于 FileManager → 软件更新** 点击“检查更新”。默认不在后台联网检查，不会自动下载安装。

检测到更高的稳定版本后，可查看更新说明，选择“下载并安装”。安装包使用 HTTPS 下载，并由内置公钥验证签名及签名内的版本号。签名不匹配、下载失败或仍有后台任务时不会继续安装。安装完成后由你点击“立即重启”；重启前等待浏览会话保存，再次检查后台任务。

修改设置中的 GitHub 展示地址，不会改变更新源和公钥。更新源固定为：

```text
https://github.com/dBevil-z/FileManager/releases/latest/download/latest.json
```

| 安装方式             | 更新行为                                                                                                                                |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| 用户安装脚本         | 下载签名 `.bin`，验证后在本人拥有的安装目录内写入临时文件并原子替换；保留 `filemanager.previous` 供手动回退。无需 sudo。                |
| AppImage             | Tauri 验证并替换当前 AppImage；原文件所在目录须可写。                                                                                   |
| deb                  | 下载签名 deb，由 `pkexec /usr/bin/dpkg -i` 请求系统授权；不在应用内收集密码。取消授权或缺少 pkexec 时显示错误，可从 Releases 手动安装。 |
| 源码构建、未识别位置 | 可以检查版本，从 Releases 手动安装或重新构建。                                                                                          |

RPM 尚未发布；对应更新适配为将来打包预留，当前不宣称 RPM 兼容。

0.2.3 及以前的“检查更新”只打开网页。请先安装 0.2.4 或更新版本，以启用真正的更新功能。选择一种安装方式，避免多个桌面入口或旧进程继续驻留。升级前保存正在编辑的文件，完成文件传输。应用保留工作空间、设置、会话和操作历史。

用户安装默认目录为 `$XDG_DATA_HOME/filemanager-desktop/bin`，未设置 XDG 时为 `~/.local/share/filemanager-desktop/bin`。回退时先完整退出程序，将 `filemanager.previous` 复制为 `filemanager`，再启动。deb/AppImage 需要从对应旧版 Release 安装。当前只有一个已发布版本时，不会凭空出现更新；页面应显示“已是最新版本”。

## 维护者发布步骤

1. 更新 `package.json`、`src-tauri/tauri.conf.json`、两个 crate 的版本、锁文件和桌面元数据，并添加 `docs/releases/v版本.md` 中英文版本说明。
2. 运行前端、Rust/native、发布脚本测试和格式检查；执行 `python3 scripts/release.py check-version --tag v版本` 确认版本一致。
3. 推送源码，创建并推送稳定版本标签，例如 `git tag -a v0.2.5 -m 'FileManager 0.2.5'`、`git push origin v0.2.5`。
4. [发布工作流](../.github/workflows/release.yml) 先执行检查，再构建 deb/AppImage/用户安装包，生成版本绑定签名、`latest.json` 与 `SHA256SUMS`。安装包齐备后才公开 Release。
5. 下载 Release 产物，核对签名、校验和与原生启动。运行 [Linux 工作流](../.github/workflows/linux.yml) 并启用 `verify_public_update`，或将 `FILEMANAGER_UPDATE_BASELINE` 指向旧版可执行文件后运行 `cargo test -p filemanager published_release_can_upgrade_an_isolated_user_install --locked -- --ignored --nocapture`。测试覆盖公开更新源、签名下载、隔离安装和同版本检查。

工作流也支持手动输入已存在的稳定标签。已公开版本禁止覆盖安装包；有修复时发布新版本。普通源码推送和 PR 只运行检查，不会发布 Release。构建路径与依赖使用官方源，仓库不配置个人镜像。

## 签名密钥

GitHub Actions 使用两个仓库加密密钥：`TAURI_SIGNING_PRIVATE_KEY`、`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。公钥保存在 Tauri 配置内，可以公开；私钥及其密码不得进入 Git、截图、日志或 Release 产物。

首次发布的本机密钥存放在仓库外 `~/.local/share/filemanager-signing/`，目录权限 0700，私钥与密码文件权限 0600。请维护者自行离线备份此目录；GitHub 不提供读取已配置 Actions 密钥的能力。**丢失私钥后，已安装客户端无法信任新密钥签名的更新**，需要安排手动迁移，不能只更换配置中的公钥。

本地签名构建从受保护的环境变量或私钥路径读取凭据；不要把密码写进命令历史。运行 `python3 scripts/release.py prepare --tag v版本` 会从现有构建生成发布目录；`verify` 校验资产、摘要、清单版本与目标匹配，它不是独立的签名验证器，运行时签名验证由 Tauri 完成。
