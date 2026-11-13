# 自动更新说明

NetVision Pro 使用 Tauri 2 官方 updater 插件和 GitHub Releases 提供 Windows 稳定版更新。更新源固定为：

```text
https://github.com/yuchen0x1/Internet-Speed-Test/releases/latest/download/latest.json
```

客户端内置 updater 公钥，更新包必须通过 Tauri updater 的签名校验；校验失败时不会安装。发布工作流另外生成 `SHA256SUMS.txt`，用于人工核对发布资产。

v1.12.0 起启用 `requireSignedVersion`：签名中的受保护版本必须与更新清单声明的版本一致。后续发布应使用锁定的 Tauri CLI 2.12+ 生成带版本签名，不接受省略版本的旧签名。v1.11.0 客户端仍可验证并安装这种新签名。

## 用户侧工作流程

1. 应用启动后按设置中的间隔检查一次稳定版 Release。默认开启，默认间隔为 24 小时；最近一次检查时间保存在当前应用 WebView 的本地存储中。
2. 发现新版本时，应用通过通知提示用户进入“设置与关于 → 关于”。
3. 点击“检查更新”可手动重试，并查看版本号、发布时间和发布说明。
4. 点击“下载并安装”后显示真实已下载字节数和总大小（更新源提供总大小时），下载完成后由 Windows NSIS passive 安装并重启应用。
5. GitHub 不可访问、Release 缺失、签名不正确或下载失败时，当前已安装版本继续运行，界面显示失败原因。

当前更新源只有稳定版。Beta/Dev 通道没有对应的发布端点和筛选策略，因此设置页不会伪装提供未实现的测试通道。

## 发布流程

正式发布使用语义化版本标签，例如 `v1.13.0`：

```powershell
git checkout main
git pull --ff-only
# 同步修改 package.json、package-lock.json、Tauri/Cargo 配置及 Cargo.lock 的项目版本
git tag -a v1.13.0 -m "NetVision Pro v1.13.0"
git push origin main
git push origin v1.13.0
```

推送 `v*` 标签后，`.github/workflows/release.yml` 会在 Windows runner 上：

- 安装锁定的 Node/Rust 工具链和依赖；
- 校验前端、Tauri、Cargo 版本及两个锁文件与标签一致；
- 运行签名构建；
- 检查 NSIS 安装包和 `.sig` 文件存在；
- 生成 `SHA256SUMS.txt` 和 Tauri `latest.json`；
- 检查更新清单含签名和 HTTPS GitHub 下载地址；
- 创建 GitHub Release 并上传资产。

如果同名 Release 已经存在，工作流会失败并保留原有资产，不会使用 `--clobber` 覆盖发布文件。

## GitHub Secrets

在仓库 Settings → Secrets and variables → Actions 中配置：

- `TAURI_SIGNING_PRIVATE_KEY`：Tauri/minisign 更新私钥；
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：私钥密码。

私钥只允许存在于 GitHub Actions Secrets 或受保护的本机密钥目录。不要把私钥、密码或构建日志复制到仓库、前端代码、Issue 或 Release 说明中。客户端只包含用于验证的公钥。

当前项目没有配置 Windows Authenticode 代码签名证书。Tauri updater 包签名已经用于防止更新包被替换，但 Windows 安装器的 SmartScreen 信任度仍受代码签名证书影响。

## 本地签名构建

在 Windows 本机准备密钥后运行：

```powershell
npm run build:release
```

脚本从 `%USERPROFILE%\\.tauri\\netvision-pro-updater.key` 和同名 `.password` 文件读取密钥，也支持设置 `TAURI_SIGNING_PRIVATE_KEY` 与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 环境变量。没有私钥时，普通构建可以运行，但不能生成可用于在线更新的签名资产。

## 验证清单

本地可执行：

```powershell
npm ci
npm run typecheck
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

有签名密钥和测试 Release 时，再验证：

- `latest.json` 的版本、说明、签名和 Windows URL；
- 安装包 SHA-256 与 `SHA256SUMS.txt`；
- 当前版本检查到新版本；
- 下载中断、文件损坏和签名错误被拒绝；
- 安装后版本更新并自动重启；
- 无更新、GitHub 不可达和旧版本响应不阻塞启动。

没有真实新版本、签名密钥或 Windows GUI 环境时，不应把这些端到端场景标记为通过。

## 已知限制

- 更新检查只针对 GitHub Releases 的稳定版；没有 Beta/Dev 通道。
- 当前 Windows 安装模式为 passive，下载完成后交给 NSIS 安装器，应用不提供独立的取消按钮。
- 更新器签名校验由 Tauri updater 负责；`SHA256SUMS.txt` 是额外的发布审计材料，不替代签名校验。
- 发布工作流不会自动创建代码签名证书，也不会在缺少 updater 私钥时伪造签名资产。
