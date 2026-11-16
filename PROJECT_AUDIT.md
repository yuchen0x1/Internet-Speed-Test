# NetVision Pro 项目现状审计

审计日期：2026-11-16  
基线分支：`main`，基线版本：`1.14.0`  
审计范围：源代码、配置、测试、CI、发布文件与已有运行记录。没有把 README 或历史报告当作唯一依据。

## 1. 项目定位

NetVision Pro 是 Windows 10/11 x64 桌面网络测速与诊断工具，面向个人用户、开发者和运维人员。它在本机执行真实的延迟、下载、上传、质量、DNS、TCP、HTTPS、路由、NAT、批量 ICMP 与流量监控测量，将结果保存到本地 SQLite；没有应用服务端、账号体系或云数据库。

主要使用场景：

- 判断实际宽带吞吐、延迟、抖动、负载延迟和丢包。
- 排查本机接口、网关、DNS、IPv6、服务连通性和路由质量。
- 批量观察多个目标的可达性。
- 长期保存历史、定时测速、发送通知并通过 Prometheus 采集。
- 通过桌面 UI 或 CLI 复用同一测速引擎。

## 2. 技术与运行架构

| 层 | 实际实现 |
| --- | --- |
| 前端 | React 19、TypeScript 5.8、Vite 6、Zustand 5 |
| 桌面壳 | Tauri 2，Windows 无边框窗口、托盘、通知、自动启动、更新 |
| 后端 | Rust 2021、Tokio、Reqwest、Tokio Tungstenite、Windows Icmp API |
| 测速引擎 | `src-tauri/src/engine/`；LibreSpeed、Cloudflare、M-Lab NDT7、节点优选、质量分析 |
| 诊断 | `diagnostics.rs`；接口、Wi-Fi、IP、DNS、网关、TCP、HTTPS、ICMP、tracert、MTR 式质量、NAT、连通性矩阵 |
| 数据 | `rusqlite` bundled SQLite 保存历史和诊断；JSON 原子保存设置 |
| 监控 | 1 秒网卡采样环形缓冲、连续 Ping、托盘速率、Prometheus 回环端点 |
| 自动化 | 计划测速、网络状态通知、告警、CLI JSON 输出 |
| 构建发布 | Vite、Tauri NSIS、GitHub Actions、minisign 更新包签名；Windows Authenticode 尚未配置 |

关键入口：

- `src/App.tsx`：启动、主题、页面路由、全局错误和键盘控制。
- `src/api.ts`：前端唯一 IPC 封装。
- `src/store/`：测速、批量拨测、诊断、设置和监控状态。
- `src/pages/`：测速、质量、诊断、历史、节点、批量拨测、监控和设置页面。
- `src-tauri/src/commands.rs`：Tauri 命令层与导出、更新、诊断入口。
- `src-tauri/src/storage.rs`：SQLite schema、迁移、历史筛选、统计和诊断历史。
- `src-tauri/src/lib.rs`：Tauri 初始化、托盘、后台任务、端点生命周期。

## 3. 已有能力清单

### 测速与质量

- 多流延迟、下载、上传和质量阶段；过程事件、样本曲线和取消。
- LibreSpeed、Cloudflare Speed、M-Lab NDT7 和自定义节点。
- 自动节点优选、节点收藏、HTTPS 校验和显式 HTTP 例外。
- Bufferbloat、抖动、丢包、HTTP 失败与质量评级。

### 诊断与监控

- 本地接口、Wi-Fi、网关、DNS、TCP、HTTPS、IPv6、ICMP、tracert、MTR 式路由、DNS 一致性、NAT、服务矩阵。
- 批量 ICMP 拨测：任务 ID、去重、输入上限、并发上限、取消、重载恢复、CSV/JSON 导出。
- 网卡流量环形缓冲、连续 Ping、网络状态变化通知和 Prometheus `/metrics`。

### 历史与系统集成

- 日期范围、排序、游标分页、统计卡、趋势、热力图、详情回放、CSV/JSON/HTML 导出。
- 历史文本/状态筛选，且列表、统计、分页和导出使用同一筛选口径。
- 失败和取消状态如实保存；缺失上传、丢包和 ICMP 结果不补零。
- 计划测速、系统通知、托盘测速、开机启动、关闭到托盘和应用内更新。
- CLI 与桌面端复用引擎并输出 JSON。
- CLI 支持实验元数据、历史 list/export、筛选和安全 CSV 输出。
- 设置版本化 JSON 备份/恢复、输入校验和保存失败回滚。
- 监控当前原始环形缓冲 CSV/JSON 导出。
- 监控当前 1 小时窗口的 1m/5m/1h 时间桶聚合摘要。
- 诊断页支持脱敏 JSON/HTML 报告导出。
- 前端 invoke、Rust 注册表和命令定义的本地 IPC 契约检查。

## 4. 当前优势

1. 测量路径真实：前端没有伪造速度，网络 I/O 集中在 Rust。
2. 数据口径清楚：成功、失败、取消、HTTP 失败、ICMP 未知分开表达。
3. 供应商适配层已经抽象，新增测速协议不需要重写页面。
4. 本地优先，公网 IP 查询、NDT7 令牌和 Prometheus 监听均有隐私边界。
5. 对竞态、取消、输入上限、历史日期边界和 CSV 公式注入已有回归测试。
6. CI 已包含 npm 审计、Rust 审计、IPC 契约、格式、Clippy、前端测试和构建。

## 5. 当前缺陷与技术债

### 已在本轮处理

- 历史页只能按日期筛选，无法按节点、运营商、接口、IP 或状态检索。
- 历史列表、统计和导出缺少统一的文本/状态筛选契约。
- 诊断页缺少一次运行基础检查的编排入口，用户需要逐项点击。
- 键盘跳转、焦点恢复、系统 reduced-motion 动态变化和页面语义仍不完整。
- 设置缺少用户主动备份、导入校验和失败回滚。
- 监控只能在窗口内查看，无法保留原始数据供外部分析。
- CLI 缺少历史查询和可重复实验元数据。
- 诊断详情缺少可脱敏的工单报告导出。

### 仍未完成

- 完整 i18n（当前文案主要为中文）。
- 监控 24 小时/7 天持久化时间桶聚合与长期趋势。
- SQLite 样本独立表、FTS5 和大历史库性能基准。
- 端到端 UI 自动化、安装器跨版本更新验收和 Windows Authenticode。
- 远端分布式拨测、云同步、插件系统和分享页。
- 全面屏幕阅读器、125%/150% DPI 和多日休眠/网卡切换验收。
- 诊断报告目前按字段名脱敏，错误/摘要自由文本仍需分享前人工复核。

## 6. 基线与本轮验证

基线检查（修改前）：

| 检查 | 结果 |
| --- | --- |
| `npm run typecheck` | PASS |
| `npm test -- --run` | PASS，11 个文件、67 项 |
| `npm run build` | PASS |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | PASS |
| `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | PASS |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml` | PASS，77 项通过、4 项真实网络用例忽略 |

本轮变更后的共享工作区检查：

| 检查 | 结果 |
| --- | --- |
| `npm run typecheck` | PASS |
| `npm test -- --run` | PASS，12 个文件、74 项 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | PASS |
| `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | PASS |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml` | PASS，92 项通过、4 项真实网络用例忽略 |
| `npm run docs:check` | PASS，发布说明同步且本地链接通过 |
| `npm run ipc:check -- --json` | PASS，64 个调用、注册和定义集合一致 |
| `npm audit --audit-level=high --registry=https://registry.npmjs.org` | PASS，0 个漏洞 |
| `cargo audit --file src-tauri/Cargo.lock --json` | PASS，返回 0 个阻断漏洞；3 项 unmaintained、1 项 unsound 告警，registry yanked 检查有超时提示 |

以上是本地代码和测试证据，不等同于公网全链路、多日运行、安装更新或发布签名的生产验收。

## 7. 成熟度判断

| 维度 | 当前判断 | 依据 |
| --- | --- | --- |
| 核心功能 | 较成熟 | 测速、诊断、历史、监控、批量拨测和 CLI 均有真实代码 |
| 用户体验 | 中上 | 页面完整、状态反馈较多，本轮补齐批量诊断和键盘路径 |
| 架构 | 中上 | Rust 分层、IPC 集中、存储迁移；`run_test` 和命令层仍偏长 |
| 性能 | 中上 | 按需拆包、环形缓冲、并发边界；缺少长期 OS 基线 |
| 安全 | 中上 | 输入校验、回环指标、令牌脱敏、签名更新；依赖维护告警仍存在 |
| 稳定性 | 中上 | 竞态和取消测试较全；缺少休眠、网络切换和安装升级矩阵 |
| 工程化 | 中上 | CI、审计、发布工作流和文档齐全；缺少 E2E 和 Authenticode |

## 8. 后续建议

优先完成历史 FTS5/样本拆分、监控聚合、Windows 安装更新验收和依赖告警处置；之后再评估多语言、远端拨测、云同步与插件生态。任何“生产级”结论都应补充真实安装、签名、跨版本更新和长时间运行证据。
