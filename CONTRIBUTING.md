# 贡献指南

感谢参与 NetVision Pro。项目是 Windows-only 的 Tauri 2 桌面应用，前端使用 React/TypeScript，系统和网络能力主要在 Rust 侧实现。

## 开发环境

- Windows 10/11 x64。
- Node.js 20.9+。
- Rust stable、MSVC 工具链和 Windows WebView2。
- Git。

安装依赖并运行开发版：

```bash
npm ci
npm run tauri dev
```

## 提交前验证

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
