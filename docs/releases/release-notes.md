# 发布说明

## v1.12.0 (2026-11-13)

- 测速历史新增今天、近 7 天和近 30 天筛选，结束日期包含全天；CSV/JSON 使用当前日期与排序。
- 统计卡汇总整个筛选范围，区分成功、失败和取消，未测上传不再按 0 参与均值。
- 修复完成事件早于结果保存、取消/失败显示完成、过期状态查询和事件连接无法重试的问题。
- 修复节点新增后丢失、删除后恢复、授权 HTTP 节点无法使用等问题；删除当前手动节点会回到自动优选。
- 改善设置失败一致性、监控暂停和 Ping 目标恢复；较旧历史回填不覆盖实时点。
- 取消测速不生成完整质量评级，未知丢包与上传在雷达图上保持未知；配置反复损坏时保留既有备份。
- 启动时按配置检查稳定版更新，显示发布说明与下载字节进度；更新签名绑定版本，拒绝不匹配的更新内容。

[发布验证](verification-v1.12.0.md) · [产物暴露面复核](exposure-v1.12.0.md)

已知限制：200ms 监控采样下，3600 点缓冲约覆盖 12 分钟；日均、趋势和热力图仍基于已加载记录。安装器未配置 Windows Authenticode 证书，更新包另有 minisign 签名。旧客户端实际更新安装及重启仍需单独验收。

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

## v1.9.0

- 提高历史数据与导出的可信度：准确计数、CSV 公式注入防护、诊断历史持久化及按类型筛选。
- 改善监控、Prometheus、测速状态恢复和设置文件写入的可靠性。
- 增加错误边界、可访问性、键盘快捷导航、历史游标分页和按需页面加载。
- 加强 Rust 格式/lint 门禁与 CI 依赖审计配置。

## v1.8.0

- 接入 M-Lab NDT7：通过 Locate API v2 动态发现节点，支持 WSS/WS 和真实 WebSocket 下载、上传。
- 完善 NDT7 令牌与端点边界：敏感信息不进入 IPC、事件、CLI JSON 或历史存储。
- 优化节点选择、测速过程展示和质量分析，并保留 LibreSpeed、Cloudflare Speed 与自定义节点能力。
