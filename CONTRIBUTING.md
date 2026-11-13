# 贡献指南

感谢参与 NetVision Pro。项目是 Windows-only 的 Tauri 2 桌面应用，前端使用 React/TypeScript，系统和网络能力主要在 Rust 侧实现。

## 开发环境

- Windows 10/11 x64。
- Node.js 20.9+。
- Rust stable、MSVC 工具链和 Windows WebView2。
- Git。

## 分支约定

- `main` 是项目唯一的长期维护分支，也是所有后续更新的目标分支。
- 开始工作前先同步 `main`：`git switch main`、`git pull --ff-only origin main`。
- 如因 GitHub PR 或隔离实验需要创建临时分支，合并到 `main` 后应立即删除本地和远程临时分支，不保留长期漂移分支。
- 发布只从 `main` 上的版本提交创建 `v*` 标签；不要从临时分支或未同步的本地提交发布。

安装依赖并运行开发版：

```bash
npm ci
npm run tauri dev
```

## 提交前验证

版本更新日志统一维护在 `docs/releases/release-notes.md`。修改后运行 `npm run docs:sync` 生成根目录 `CHANGELOG.md`，再运行 `npm run docs:check` 检查镜像、版本锚点与本地链接；CI 同样检查，避免两份日志漂移。README 的“最新更新”只保留最新一个版本块，未发布内容必须明确标注。

修改前端：

```bash
npm run typecheck
npm run test
npm run build
```

修改 Rust：

```bash
cargo fmt --all -- --check
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

需要验证真实网络行为时，再显式执行被忽略的集成测试：

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml -- --ignored
```

构建安装包：

```bash
npm run tauri build
```

不要把本机测速截图、导出的历史数据库、`settings.json`、公网 IP、MAC 地址或其他个人网络信息提交到仓库。

## 本地文件与清理

- 提交前检查 `git status --short` 和暂存区差异；不要对包含本地验证数据的目录直接执行 `git add -f`。
- `node_modules/` 是开发依赖，`dist/`、`src-tauri/target/debug/` 和 `src-tauri/target/release/` 是可重建产物。依赖安装和构建流程会重新生成它们。
- 清理构建产物前，先停止本项目的构建或调试任务，并核实目录内没有手动保存的数据。其他 `target/` 子目录可能包含隔离验收的数据库、设置及备份，不能整目录盲删。
- `.autonomous-upgrade/`、`.evolution/` 和驱动生成的 `NVIDIA Corporation/` 只用于本地；过期的临时报告、下载副本及验证脚本在核实用途后清理。
- 构建目录清理时可保留最新 `src-tauri/target/release/netvision-pro.exe` 和仍需使用的安装包。隔离数据库、设置及恢复副本单独保留；正式 `%APPDATA%` 数据目录不属于项目垃圾清理范围。
- 本地签名操作说明与密钥用途核实后再处理，不因它们位于临时目录就删除。优先把确认无用的文件移入 Windows 回收站，保留恢复机会。
- `release-artifacts/`、根目录 `release-notes.txt` 是发布工作流的临时输出，测试覆盖率、浏览器报告和 `.nvwork/` 也不入库。GitHub Release 中的安装包、签名、更新清单和校验文件是正式分发资产，应保留。
- `.env`、私钥、签名密码、数据库、应用设置及恢复备份不入库；清理时保留仍需使用的配置、历史和签名资料。
- `docs/screenshots/` 只提交已人工检查、已脱敏并以 `-verified.png` 结尾的图片。仅修改文件名不等于完成脱敏。
- 保留当前使用指南、配置说明、版本更新日志、发布验证和必要的历史依据；过期审计快照及已被后续报告取代的临时计划不作为当前待办。
- 已整合的旧测试汇总和重复日志可从固定历史提交查阅，不必在工作目录再维护一份。本轮清理不重写 Git 历史，也不自动删除远端 Release 或附件。

## 代码约定

- 保持网络 I/O 在 Rust/Tauri 命令层，前端通过 `src/api.ts` 调用。
- 对用户输入的 URL、主机名和端口在边界处校验；自定义测速节点必须保持 HTTPS 约束。
- 真实测量结果与失败、超时、未知状态分开表示，不用默认值掩盖检测失败。
- 新增设置时同步更新 Rust 默认值、序列化测试、设置页和配置文档。
- 新增页面功能时同步补充对应单测或集成测试，避免只依赖手工验证。
- 不引入与任务无关的重构、依赖升级或 UI 改版。

## 提交 Issue

请提供：

- 应用版本、Windows 版本和安装方式。
- 可复现步骤和预期/实际结果。
- 相关页面、协议或节点。
- 脱敏后的错误信息和必要截图。

网络问题请尽量说明是测速、ICMP、DNS、TCP、HTTPS、IPv6、NAT 还是路由追踪失败。不要上传个人网络标识或完整配置文件。

## 提交 Pull Request

1. 从最新 `main` 创建分支。
2. 让每个 PR 聚焦一个问题或功能。
3. 更新相关测试和文档。
4. 在 PR 描述中写明验证命令和未覆盖的真实网络场景。
5. 确认没有提交构建产物、密钥、历史数据库或敏感截图。

## 安全问题

不要在公开 Issue 中发布可利用的安全细节、令牌或敏感数据。请先联系仓库维护者，提供最小复现、受影响版本和修复建议。

## 许可证

提交代码即表示同意在项目 MIT License 下发布。新增第三方代码或资源时，请在提交说明中注明来源和许可证。
