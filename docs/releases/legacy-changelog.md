# 原变更日志归档

归档日期：2026-11-13。本文件保留本次整理前 CHANGELOG 的开发与验证明细，作为历史资料；测试段中的“未运行”等描述只指当时记录阶段。版本归属、升级方式和正式发布时间以 [完整更新日志](release-notes.md) 为准。

## v1.12.0 (2026-11-13)

### Added

- 历史页新增今天、近 7 天、近 30 天快捷筛选，刷新、失败重试和导出中状态。
- 统计卡汇总整个日期范围的成功测试，并显示完成、失败和取消数量，不受列表分页限制。
- 启动后按用户设置间隔自动检查稳定版更新，并在发现新版本时提示进入设置页。
- 设置页显示发布时间、发布说明和真实下载字节进度，可配置自动检查开关与间隔。
- 新增 [自动更新文档](../AUTO_UPDATE.md)，说明签名密钥、发布标签、验证流程和已知限制。

### Changed

- CSV/JSON 导出遵循当前日期与排序；未传筛选的旧调用继续导出全部历史（最多 5000 条）。
- 未获得的上传速度不再作为 0 参与均值；历史图表明确标注已加载范围，趋势始终按时间向前绘制。
- Release 工作流校验版本文件与标签一致性；同名 Release 已存在时停止发布，避免覆盖已有资产。

### Fixed

- 二次复查修复设置失败一致性、旧快照覆盖节点、事件绑定重试与测速对账竞态；普通设置保存不再无条件操作系统自启动。
- 修复取消测试被评为优秀、未知丢包雷达满分、HTTP 授权节点无法手动使用，以及监控暂停/Ping 目标与回填状态错误。
- 多次配置损坏会保留已有备份，后续备份使用 `.bak.1` 等序号。详细验证与剩余不足见 [二次复查报告](../recheck-2026-11-13.md)。
- 修复结束日期排除当天记录的问题，列表、计数、汇总、导出使用一致的日期边界。
- 测速先完成历史写入再通知界面刷新，失败/取消也回传完整结果；写库失败单独提示。
- 失败和取消不再显示 100% 完成或将全部阶段标成完成；计时跨阶段连续，采样缓存满后时间仍递增。

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
