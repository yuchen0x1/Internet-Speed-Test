# NetVision Pro 开源竞品与技术对标

研究日期：2026-11-16  
方法：优先查看官方 GitHub 仓库的 README、许可证、目录或核心文件、Release 和近期 commit；已有项目记录中的源码阅读结果也纳入，但没有运行竞品代码或复制 GPL/LGPL 代码。结论按“已读源码/文档”“仅元数据”区分。

## 1. 研究项目总览

本轮研究 15 个开源项目，覆盖直接测速、持续延迟监测、带宽基准、网络监控、指标生态和桌面工程：

| 项目 | 仓库 | 许可证 | 主要参考点 | 研究证据 |
| --- | --- | --- | --- | --- |
| LibreSpeed | [librespeed/speedtest](https://github.com/librespeed/speedtest) | LGPL-3.0 | 多点测速、稳定性测试、阈值告警、CSV、Web Worker | README、`speedtest_worker.js`、`stability.html`、Release |
| MySpeed | [gnmyt/MySpeed](https://github.com/gnmyt/MySpeed) | MIT | 定时测试、Cron、保留策略、多服务、Prometheus/Grafana、通知 | README、Prometheus 路由、近期依赖更新 |
| Speedtest Tracker | [alexjustesen/speedtest-tracker](https://github.com/alexjustesen/speedtest-tracker) | MIT | 历史趋势、自动测试、API、Webhook、失败状态 | README、API 路由、Release v1.15.1 |
| gping | [orf/gping](https://github.com/orf/gping) | MIT | 连续 Ping、缺失样本、终端时序图、Windows 分发 | README、`plot_data.rs`、Release |
| Netronome | [autobrr/netronome](https://github.com/autobrr/netronome) | GPL-2.0-or-later | 服务器监控、任务并发、丢包、数据库迁移、文档 | README、`packetloss.go`、Release v0.16.0、近期 migration commit |
| speedtest-cli | [sivel/speedtest-cli](https://github.com/sivel/speedtest-cli) | Apache-2.0 | CLI 参数、JSON/CSV、脚本集成 | README、CLI 输出和 Release |
| M-Lab ndt-server | [m-lab/ndt-server](https://github.com/m-lab/ndt-server) | Apache-2.0 | NDT5/NDT7 服务端、可观测性、协议实现 | README、Go 源码、Release v0.26.1 |
| M-Lab ndt | [m-lab/ndt](https://github.com/m-lab/ndt) | Apache-2.0 | NDT 协议客户端/服务端生态 | README、仓库结构 |
| iperf3 | [esnet/iperf](https://github.com/esnet/iperf) | BSD-style | TCP/UDP/SCTP 吞吐、参数化基准、JSON 输出 | README、命令行接口、Release 3.22 |
| Cloudflare Speedtest | [cloudflare/speedtest](https://github.com/cloudflare/speedtest) | MIT | 可嵌入测速引擎、延迟/吞吐/质量测量 | README、NPM 入口、Release v1.17.0 |
| OpenSpeedTest | [OpenSpeedTest/SpeedTest](https://github.com/openspeedtest/SpeedTest) | MIT | 轻量 Web 测速、部署简单、多平台浏览器体验 | README、公开部署说明 |
| SmokePing | [oetiker/SmokePing](https://github.com/oetiker/SmokePing) | GPL-2.0 | 长期延迟趋势、告警、RRD 数据归档 | README、监控配置模型 |
| Prometheus | [prometheus/prometheus](https://github.com/prometheus/prometheus) | Apache-2.0 | 指标命名、抓取、时间序列和告警生态 | 官方文档、exporter 约定 |
| Tauri | [tauri-apps/tauri](https://github.com/tauri-apps/tauri) | MIT/Apache-2.0 | 桌面窗口、安全边界、更新和跨平台工程 | 官方文档、仓库结构 |
| Tauri Plugins | [tauri-apps/plugins-workspace](https://github.com/tauri-apps/plugins-workspace) | MIT/Apache-2.0 | 自动启动、通知、更新、对话框插件 | 插件目录、近期 CI/版本更新 |

近期维护状态不能只看 Release：例如 MySpeed 的最新稳定 Release 较早，但 2026 年仍有依赖更新 commit；Netronome、LibreSpeed、Cloudflare Speedtest、iperf3、M-Lab 和 Tauri 插件在 2026 年仍能看到近期 commit 或 Release。未把星数当作质量证明。

## 2. 重点对标

### 2.1 LibreSpeed

- **对方优势**：稳定性测试是独立产品流程，持续采样延迟、失败率、阈值告警和 CSV 导出；多点配置和 Web Worker 让浏览器端长期运行更直接。
- **实现原因**：测试循环、worker 生命周期和 UI 图表解耦，结果以样本序列驱动，而不是只展示最终平均值。
- **NetVision Pro 现状**：已有连续 Ping、质量阶段、历史和批量拨测；本轮补充历史搜索和批量基础诊断，但缺少独立的“稳定性测试”专页和阈值区间导出。
- **借鉴决策**：重新实现样本/阈值/导出契约，不复制 LGPL 代码；列入 EV-021、EV-022。
- **我们的优势**：原生 Windows ICMP、Rust 多协议、诊断深度和本地隐私边界更强。

### 2.2 MySpeed

- **对方优势**：定时测速、Cron、保留期、多服务选择、健康通知、Prometheus/Grafana 和历史统计形成完整长期观测闭环。
- **实现原因**：服务端持久化、调度器和监控接口以长期运行作为一等场景。
- **NetVision Pro 现状**：已有计划测速、保留策略、通知和 Prometheus，但没有设置备份、跨设备同步或完整健康通知渠道。
- **借鉴决策**：优先补设置备份、监控聚合和告警状态；云同步和多渠道通知保持 P3。
- **我们的优势**：单机安装更轻，测速引擎和桌面交互不需要服务器。

### 2.3 Speedtest Tracker

- **对方优势**：历史趋势、公开 API、Webhook 和失败状态适合家庭服务器长期运行。
- **实现原因**：测试、存储、API 和通知边界清晰，数据可被其他系统消费。
- **NetVision Pro 现状**：有 CLI JSON、Prometheus、CSV/JSON/HTML 导出，但没有稳定的版本化历史 API。
- **借鉴决策**：先稳定 CLI/导出 schema，再考虑只读本地 API；不引入服务端。

### 2.4 gping

- **对方优势**：连续时序图把每个 RTT 和超时都直接展示，缺失值不会被画成 0。
- **实现原因**：数据模型保留 `Option`，渲染层对断点有明确语义。
- **NetVision Pro 现状**：连续 Ping 和批量拨测已保留未知样本；本轮历史状态筛选与诊断批量流程沿用这种诚实表达。
- **借鉴决策**：继续使用缺失值断线、状态分层和真实包进度。

### 2.5 Netronome

- **对方优势**：把网络测速和服务器监控结合，重视任务隔离、数据库迁移和丢包统计。
- **实现原因**：后台任务有明确生命周期、并发边界和可恢复的数据状态。
- **NetVision Pro 现状**：批量拨测已经有 run_id/revision、取消和 8 路并发；监控历史仍是内存环形缓冲。
- **借鉴决策**：把任务状态机和迁移思路用于监控聚合；不复制 GPL 实现。

### 2.6 iperf3

- **对方优势**：参数化 TCP/UDP/SCTP 基准适合可重复实验，JSON 便于自动化。
- **实现原因**：测试参数、协议、方向、窗口和输出格式明确分层。
- **NetVision Pro 现状**：多流 HTTP/WebSocket 更接近普通用户端到端体验；没有 UDP/SCTP 专业基准。
- **借鉴决策**：保留 HTTP/WebSocket 真实性；为 CLI 增加可复现参数和测量元数据，不直接嵌入 iperf3。

### 2.7 Cloudflare Speedtest

- **对方优势**：提供可嵌入 JavaScript 测量引擎，覆盖延迟、下载、上传和质量。
- **实现原因**：引擎可以被网页或产品复用，接口对测量阶段做了清楚抽象。
- **NetVision Pro 现状**：已有 Cloudflare 适配器，并把前端网络访问限制在 Rust。
- **借鉴决策**：参考阶段和结果模型；不绕过第三方服务条款抓取网页。

### 2.8 M-Lab NDT

- **对方优势**：NDT7 有公开协议、测量服务和研究数据生态。
- **实现原因**：Locate、WebSocket 子协议和服务端模型配合，允许不同客户端复用。
- **NetVision Pro 现状**：已接入 Locate、WSS 优先和运行时令牌脱敏。
- **借鉴决策**：继续做节点可用性、协议错误和令牌生命周期测试；不把握手 RTT 当作 ICMP 延迟。

## 3. 维度对比表

| 维度 | 领先项目做法 | NetVision Pro 当前状态 | 差距与行动 |
| --- | --- | --- | --- |
| 核心测速 | LibreSpeed/Cloudflare 多阶段与稳定性 | Rust 多协议、多流、质量阶段 | 补独立稳定性模式和更长时序导出 |
| 长期观测 | MySpeed/SmokePing/Speedtest Tracker | 计划测速、历史、Prometheus | 增加按小时/天聚合、健康状态和备份 |
| 连续延迟 | gping 缺失样本断点 | 连续 Ping、批量拨测 | 统一时间桶、导出和阈值标记 |
| 带宽基准 | iperf3 参数化协议测试 | HTTP/WebSocket 面向终端用户 | CLI 增加实验元数据；UDP 作为 P3 研究 |
| 监控生态 | Prometheus/Grafana exporter | 回环 `/metrics`、真实 Gauge | 补文档示例和指标新鲜度说明 |
| 诊断深度 | Netronome 关注监控与丢包 | DNS/NAT/MTR/服务矩阵已覆盖 | 补批量流程、报告和长期故障关联 |
| 桌面体验 | Tauri 官方插件 | 托盘、通知、更新、自动启动 | 补 E2E、安装验收、DPI/读屏验证 |
| 隐私安全 | 本地部署项目可自托管 | 本地优先、回环指标、令牌脱敏 | 补设置备份加密选项和安全恢复文档 |
| 许可证 | MIT/Apache 易于重实现；GPL/LGPL 有义务 | 未新增第三方运行依赖 | 继续只借鉴接口和思路，保留 NOTICE 记录 |

## 4. 明确保持的差异

- 不直接逆向或调用 Ookla/FAST 私有端点；应用只提供固定官方浏览器入口。
- 不把网卡协商速率当作公网测速值。
- 不把 ICMP 禁用、HTTP 失败或缺失上传结果伪装成零。
- 不为了竞品数量引入服务端、账号、云数据库或新的高维护依赖。
- 不复制 GPL/LGPL 源码；需要相同能力时按现有架构重新实现并记录许可证评估。

## 5. 结论

最值得落地的组合是：LibreSpeed 的稳定性样本语义、MySpeed/SmokePing 的长期聚合、gping 的缺失值表达、Netronome 的任务生命周期、iperf3 的 CLI 可重复性、Prometheus 的指标契约和 Tauri 的桌面生命周期。NetVision Pro 在本地真实测量、Windows 诊断深度和隐私边界上保持差异化。
