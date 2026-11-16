# NetVision Pro 项目自主升级报告

报告日期：2026-11-16  
当前版本：1.14.0（本次发布）

## A. 竞品研究结论

本轮研究 15 个相关开源项目：LibreSpeed、MySpeed、Speedtest Tracker、gping、Netronome、speedtest-cli、M-Lab ndt-server、M-Lab ndt、iperf3、Cloudflare Speedtest、OpenSpeedTest、SmokePing、Prometheus、Tauri 和 Tauri Plugins。重点源码/文档、许可证、Release 和近期提交记录见 [COMPETITOR_ANALYSIS.md](COMPETITOR_ANALYSIS.md)。

最值得借鉴的能力：

- LibreSpeed：稳定性测试、缺失样本、阈值标记和 CSV 导出。
- MySpeed/SmokePing：长期定时测试、保留策略、聚合趋势和 Prometheus/Grafana。
- gping：连续 RTT 时序和超时断点的诚实表达。
- Netronome：任务生命周期、并发边界、迁移和丢包监控。
- iperf3/speedtest-cli：可重复 CLI 参数和机器可读输出。
- Tauri 官方插件：桌面生命周期、通知、自动启动、更新和对话框。

NetVision Pro 的差异化优势是 Windows 原生 ICMP、Rust 多协议测速、DNS/NAT/MTR 诊断、本地优先数据边界，以及不把未知结果伪装为零。没有直接复制 GPL/LGPL 源码，也没有为研究引入新的运行时依赖。

## B. 升级数量与状态

路线图共记录 **42 个**独立升级点：

| 状态 | 数量 | 编号 |
| --- | ---: | --- |
| 已完成 | 19 | EV-001～EV-014、EV-017、EV-023、EV-024、EV-034、EV-036 |
| 部分完成/已有基础但仍需独立验收 | 1 | EV-016 |
| 未完成 | 22 | EV-015、EV-018～EV-022、EV-025～EV-033、EV-035、EV-037～EV-042 |

已完成项：

1. 历史文本搜索。
2. 历史状态筛选。
3. 列表、统计、分页和导出共享筛选口径。
4. 搜索长度限制、参数绑定和 LIKE 通配符转义。
5. 诊断页一键基础检查。
6. 诊断批量任务的并发上限和失败聚合。
7. Ctrl/Cmd+Enter 开始/停止测速、Escape 取消。
8. skip link、main landmark 和切页焦点恢复。
9. 系统 reduced-motion 动态监听。
10. 网络状态、加载、Toast、进度和导航的 ARIA 语义。
11. 页面标题随当前页面更新。
12. 设置备份导出。
13. 设置恢复前校验。
14. 设置恢复失败回滚。
15. 监控环形缓冲的 CSV/JSON 时间窗导出。
16. CLI 可重复实验元数据。
17. CLI 历史 list/export 命令。
18. 诊断 JSON/HTML 报告导出、结构化敏感字段脱敏和 HTML 转义。
19. 本地 IPC 命令契约检查。

补充交付：页面级 Ctrl/Cmd+1…8 快捷键，以及文档、审计和路线图同步。

EV-016 已部分完成：当前 1 小时原始环形缓冲已支持 1m/5m/1h 聚合和 P50/P95/峰值，但 24 小时、7 天的持久化趋势仍未实现。

未完成项的主要原因：安装更新、Authenticode、长期持久化聚合、长时间运行、DPI/读屏、样本拆表、FTS5、远端探针和云同步需要独立的数据迁移、外部环境或发布资源，不能只凭源码存在宣布完成。

## C. 代码修改

本轮共享工作区已修改或新增：

- `src-tauri/src/storage.rs`：历史查询过滤、状态校验、搜索边界、列表/统计统一口径和回归测试。
- `src-tauri/src/commands.rs`：历史 IPC 接收 query/status 并传递到列表、计数、统计和导出。
- `src/api.ts`、`src/types.ts`、`src/store/ui.ts`：前端历史筛选契约和跨页 UI 状态。
- `src/pages/HistoryPage.tsx`、`src/pages/HistoryPage.test.tsx`：搜索输入、状态选择、筛选导出和空状态。
- `src/api.test.ts`：扩展 IPC 参数断言。
- `src/lib/diagnostics.ts`、`src/lib/diagnostics.test.ts`：有界并发诊断编排。
- `src/pages/DiagnosticsPage.tsx`：一键基础检查、进度条和失败汇总。
- `src-tauri/src/monitor.rs`、`src/pages/MonitorPage.tsx`：监控原始时间窗 CSV/JSON 导出。
- `src-tauri/src/monitor.rs`、`src/api.ts`、`src/pages/MonitorPage.tsx`：当前窗口时间桶聚合和 P95 摘要。
- `src-tauri/src/cli.rs`：CLI 实验元数据、历史 list/export 和安全 CSV 输出。
- `src-tauri/src/commands.rs`、`src/pages/DiagnosticsPage.tsx`：诊断 JSON/HTML 报告和脱敏写盘。
- `src/pages/SettingsPage.tsx`、`src-tauri/src/commands.rs`：版本化设置备份、恢复校验和回滚。
- `scripts/check-ipc-contract.mjs`、`package.json`：前端 invoke、Rust 注册表和命令定义的契约检查。
- `.github/workflows/ci.yml`：把 IPC 契约检查接入 Windows CI。
- `src/App.tsx`、`src/components/Sidebar.tsx`、`src/components/TitleBar.tsx`、`src/components/ui.tsx`、`src/lib/motion.ts`、`src/styles/global.css`：键盘路径、焦点、语义和动效偏好。
- `PROJECT_AUDIT.md`、`COMPETITOR_ANALYSIS.md`、`UPGRADE_ROADMAP.md`、本报告：审计、竞品、路线图和交付记录。

本轮没有数据库 schema 迁移、没有删除用户文件、没有新增生产依赖；安装包和更新资产由发布工作流生成。

## D. 测试与验证

### 本轮共享工作区实际结果

| 检查 | 状态 | 证据 |
| --- | --- | --- |
| `npm run typecheck` | PASS | TypeScript 检查退出 0 |
| `npm test -- --run` | PASS | 12 个文件、74 项通过 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | PASS | 退出 0 |
| `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | PASS | 退出 0 |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml` | PASS | 92 项通过，4 项真实网络用例忽略 |
| `npm run build` | PASS | `tsc --noEmit` 与 Vite production build 均通过，退出 0 |
| `npm run ipc:check -- --json` | PASS | 64 个前端调用、64 个注册命令、64 个 Rust 定义，三组集合无差异 |
| `npm run docs:check` | PASS | 发布说明同步，14 个版本和本地链接检查通过 |
| `npm audit --audit-level=high --registry=https://registry.npmjs.org` | PASS | 0 个漏洞 |
| `cargo audit --file src-tauri/Cargo.lock --json` | PASS | 返回 0 个阻断漏洞；保留 3 项 unmaintained 和 1 项 unsound 告警，registry yanked 检查有超时提示 |
| `npm run tauri build` | 未执行 | 未做新的安装器构建 |
| Windows NSIS 安装/更新 E2E | BLOCKED/未执行 | 需要隔离安装目录、签名资产和旧客户端 |
| 公网全链路/长期运行 | PARTIAL | 现有 ignored 测试未覆盖所有运营商、休眠和网卡切换 |

新增测试覆盖：

- 搜索和状态筛选的列表、计数、统计共享口径。
- 超长查询和非法状态拒绝。
- 历史页把筛选传递到列表、统计和导出。
- 诊断批量并发上限、失败后继续、空任务和并发归一化。
- 设置备份 envelope、字段校验、无效导入保护和恢复路径。
- 监控 CSV/JSON 导出的时间窗、字段和 API 参数。
- 监控 1m/5m/1h 聚合、均值、P50、P95、峰值和样本数。
- CLI 参数解析、实验元数据、历史查询/导出和 CSV 公式防护。
- 诊断报告字段脱敏、未知值保留、HTML 转义和 2 MiB 限制。
- IPC 契约检查的前端调用、Rust 注册与定义集合一致性。

## E. 升级前后变化

升级前：

- 历史页有日期、排序、分页和导出，但无法快速按节点/ISP/IP/接口检索。
- 诊断页面功能丰富，但基础检查需要逐项点击。
- 键盘用户缺少跳过导航、切页焦点恢复和全局测速快捷键。
- reduced-motion 只在渲染时读取，系统偏好运行中改变不会立即生效。
- 设置只有自动损坏备份，用户不能主动导出可迁移配置；监控数据只能在窗口内查看。

升级后：

- 历史搜索和状态筛选在 Rust/IPC/React 四层贯通，并且列表、统计、分页和导出使用同一口径。
- 一键基础诊断以最多 3 路并发运行 7 个真实检查，失败项单独记录，不阻塞后续项目。
- 主要页面拥有键盘快捷键、跳过链接、主体焦点、动态标题、ARIA 状态和系统动效偏好响应。
- 设置可导出带 schema 版本的 JSON，并在恢复前校验节点、范围和重复项；保存失败时保留旧设置。
- 监控页可导出当前 10 秒至 1 小时原始环形缓冲为 CSV/JSON，保留接口、速率和累计值。
- 监控页基于同一原始缓冲展示 1m/5m/1h 时间桶摘要，但还没有 24 小时/7 天持久化数据。
- CLI JSON 带实验版本、协议/节点标识、并发和时长；`history list/export` 支持日期、关键词、状态和 CSV/JSON。
- 诊断页可以导出脱敏 JSON/HTML，保留失败原因和未知值；字段名脱敏不扫描自由文本摘要。
- `npm run ipc:check` 可在构建前发现 IPC 命令重命名、漏注册或漏实现。
- 前端回归从 67 项增加到 74 项，Rust 回归从 77 项通过增加到 92 项通过；忽略项仍明确为真实网络测试。

仍然不足：

- 安装更新、Authenticode 和完整 Windows E2E 尚未完成。
- 历史大库性能、24 小时/7 天监控持久化聚合、休眠/网卡切换和 DPI/读屏尚未取得证据。
- 真实公网结果受节点、运营商、防火墙和 ICMP 策略影响，不能由本地单元测试替代。
- 诊断报告按结构化字段脱敏；错误/摘要中的自由文本不会做内容级敏感字符串扫描，分享前仍需人工复核。

## F. 下一阶段

优先完成 EV-015 和 EV-016 剩余部分：更新安装验收与 24 小时/7 天监控持久化聚合。随后做 EV-018～EV-022、EV-025～EV-033、EV-035 和 EV-037～EV-042。多语言、插件、远端探针和云同步应在数据模型、隐私和运维成本明确后再实施。

## G. 交付边界

本报告描述的是本次 v1.14.0 发布所依据的本地代码与验证证据；GitHub Actions、公开 Release 资产和在线更新清单的最终状态以发布后复核为准。
