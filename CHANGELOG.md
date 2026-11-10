# Changelog

## v1.11.0 (2026-11-10)

### Added

- 应用内自动更新：设置页「检查更新」，通过 GitHub Release 的 `latest.json` 检查新版本，minisign 签名校验后显示下载进度并以 passive 模式安装、自动重启。
- Release 自动化工作流：推送 `v*` 标签自动完成签名构建，生成 `latest.json` 与 `SHA256SUMS.txt` 并发布 GitHub Release。

### Changed

- 构建启用 `createUpdaterArtifacts`；更新器端点指向本仓库 Releases。

## v1.10.0 (2026-11-10)

### Added

- 增加测速历史 keyset 游标分页与“加载更多”，支持按日期筛选、升降序稳定翻页。
- 增加诊断历史按类型筛选。
- 增加 React 页面错误边界、Modal ARIA/键盘焦点管理与 Sidebar Ctrl+1..8 快捷导航。
- 增加监控暂停状态恢复，并在关于页面显示设置文件路径。
- 增加 CI 中 Rust fmt/严格 Clippy、前端/Rust 测试、npm/cargo audit 门禁。

### Changed

- 历史计数与 500 条窗口提示明确区分已加载行与筛选总数。
- CSV 文本字段执行 RFC4180 转义及公式注入中和(包含 `= + - @`、控制字符和前导空白场景)。
- 监控刷新间隔、采样窗口、Prometheus 实际监听状态和诊断历史记录持久化与设置行为一致。
- NDT7 节点发现缓存引入 TTL/失效路径；测速状态在应用启动和重新聚焦时重新对账。
- CI 固定 Rust 1.99.1 / Node 22.22.2；npm lockfile 下载 URL 指向官方 npm registry，版本和 integrity 未变。
- 7 个非首屏页面按需加载；增加全局 focus-visible 和 reduced-motion 兼容样式。
- 清理 Rust 格式与 Clippy 警告；`EngineEvent::Finished` 的 Rust 字段使用 `Box<TestResult>`，Serde JSON 外部结构不变。

### Fixed

- 修复公式注入 CSV 单元格的中和边界及测试断言。
- 修复历史大数据集无法继续浏览的问题，并用 ID 游标避免 offset 在并发插入/删除后漂移。
- 修复监控页面重挂时暂停状态与后端状态不一致。

### Verification

- Rust tests：59 passed，3 ignored(真实网络用例)。
- Frontend tests：37 passed；TypeScript typecheck、Vite production build 通过。
- `cargo fmt --check` 与 `cargo clippy --all-targets -- -D warnings` 通过。
- `npm audit`(官方 registry)：0 vulnerabilities。
- 未运行 GUI、真实网络验收、远端 GitHub Actions 或 `cargo audit`；详情见 `TEST_REPORT.md`。
