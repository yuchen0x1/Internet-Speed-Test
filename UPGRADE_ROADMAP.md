# NetVision Pro 自主升级路线图

制定日期：2026-11-16  
规则：每个 EV 项目都有独立价值、落点和验收标准；“已完成”只表示本地代码与相应检查有证据，不表示发布、安装、签名或公网 E2E 已完成。

## P0/P1：核心可信度、可用性和安全

| ID | 升级名称 | 模块 | 当前问题/依据 | 对标参考 | 实施方案 | 预期效果 | 文件/模块 | 难度 | 优先级 | 验收标准 | 状态 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| EV-001 | 历史文本搜索 | 历史 | 只能按日期筛选，节点/IP难定位 | MySpeed、Speedtest Tracker | 对名称、供应商、URL、接口、IP、ISP做参数化 LIKE | 故障排查更快 | storage/commands/api/HistoryPage | 中 | P1 | 输入关键词只返回匹配行，分页和统计一致 | 已完成 |
| EV-002 | 历史状态筛选 | 历史 | 完成/失败/取消混在一起 | Speedtest Tracker | 增加受限状态枚举并共享 WHERE | 可只看失败或取消 | storage/commands/api/HistoryPage | 低 | P1 | 三种状态各自计数、列表和导出一致 | 已完成 |
| EV-003 | 统一筛选统计与导出 | 历史 | 统计、列表、导出容易使用不同口径 | MySpeed | 统一 query/status/date 契约 | 避免用户看到不一致数据 | storage/commands/HistoryPage | 中 | P1 | 同一筛选下 total、summary、CSV/JSON 行数相同 | 已完成 |
| EV-004 | 搜索输入边界与通配符保护 | 安全/存储 | 任意长搜索可能放大查询，`%` 有意外语义 | Prometheus 输入边界、SQLite 参数化 | 120 字符上限、LIKE 转义、参数绑定 | 降低扫描和注入风险 | storage.rs | 低 | P0 | 超长/非法状态拒绝；`%`/`_`按字面搜索 | 已完成 |
| EV-005 | 基础诊断一键编排 | 诊断 | 用户必须逐项点击基础检查 | Netronome、MySpeed health check | 以任务列表编排本地/DNS/网关/TCP/HTTPS/IPv6/服务矩阵 | 新手排障成本下降 | DiagnosticsPage、diagnostics.ts | 中 | P1 | 一次点击运行 7 项并显示每项状态 | 已完成 |
| EV-006 | 诊断并发上限与失败聚合 | 诊断 | 批量操作可能无界并发或首个失败中断 | Netronome | 3 路 worker、失败记录、继续执行 | 稳定、结果完整 | diagnostics.ts | 低 | P1 | 并发峰值≤3；单项失败不阻断剩余任务 | 已完成 |
| EV-007 | 全局测速快捷键 | 交互 | 键盘用户需切页才能开始/取消 | Tauri 桌面交互 | Ctrl/Cmd+Enter 开始/停止，Escape 取消 | 提升效率 | App.tsx | 低 | P1 | 输入框内不拦截；运行状态正确切换 | 已完成 |
| EV-008 | 跳过导航与页面焦点恢复 | 可访问性 | 切页后焦点可能留在已卸载控件 | WAI-ARIA、桌面应用惯例 | skip link、main landmark、切页聚焦 | 键盘/读屏路径清晰 | App.tsx、global.css | 低 | P1 | Tab 可跳到主体；切页后焦点在 main | 已完成 |
| EV-009 | 动态 reduced-motion | 可访问性 | 只读取一次系统偏好 | CSS prefers-reduced-motion | 监听媒体查询变化并与应用设置合并 | 运行中切换系统偏好即时生效 | lib/motion.ts | 低 | P1 | 系统开关改变后动效立即更新 | 已完成 |
| EV-010 | ARIA 状态与加载语义 | 可访问性 | 网络状态、Toast、Suspense 缺少语义 | WAI-ARIA live region | banner/status/alert/progress/aria-current | 读屏反馈可用 | App、TitleBar、ui、Diagnostics | 低 | P1 | 关键状态通过辅助技术语义暴露 | 已完成 |
| EV-011 | 页面标题同步 | 可用性 | 多页面窗口标题相同 | 桌面应用惯例 | 页面切换更新 document.title | 多窗口/任务栏识别更清楚 | App.tsx | 低 | P1 | 8 个页面标题均正确 | 已完成 |
| EV-012 | 设置备份导出 | 数据安全 | 设置损坏时只能自动备份，不能主动迁移 | MySpeed 配置、Tauri dialog | 版本化 JSON envelope + 保存对话框 | 重装/迁移更安全 | commands/settings/api/SettingsPage | 中 | P1 | 导出文件包含 schema/version，路径可选 | 已完成 |
| EV-013 | 设置恢复前校验 | 数据安全 | 直接导入可能覆盖有效设置 | 原子配置写入 | 校验版本、字段范围、节点 URL，再原子保存 | 降低坏配置风险 | settings.rs/commands.rs | 中 | P0 | 无效文件不改变当前设置；错误可见 | 已完成 |
| EV-014 | 设置恢复回滚 | 稳定性 | 保存失败时需要恢复旧文件和自启动状态 | 现有 `save_with_autostart` | 复用事务式保存和系统状态补偿 | 恢复中断不丢设置 | settings.rs | 中 | P0 | 模拟磁盘/自启动失败，旧设置保持 | 已完成 |
| EV-015 | 更新安装 E2E | 发布 | 当前只有配置、签名和本地构建证据 | Tauri updater | Windows 隔离目录安装旧版，发现/验签/安装/重启 | 真实更新可信 | release workflow、docs | 高 | P0 | 旧客户端完成发现、验签、安装并启动新版本 | 待实施 |

## P1/P2：观测、数据和性能

| ID | 升级名称 | 模块 | 当前问题/依据 | 对标参考 | 实施方案 | 预期效果 | 文件/模块 | 难度 | 优先级 | 验收标准 | 状态 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| EV-016 | 监控时间桶聚合 | 监控 | 1 秒环形缓冲不适合长期趋势 | SmokePing、MySpeed | 对当前原始缓冲按 1m/5m/1h 聚合均值、P50、P95、峰值，并在监控页展示摘要 | 当前窗口趋势更省重绘成本 | monitor/commands/api/MonitorPage | 中 | P1 | 当前 1 小时窗口聚合口径可复现；24 小时、7 天持久化仍需后续 | 部分完成 |
| EV-017 | 监控数据导出 | 监控 | 监控只能在窗口内看 | LibreSpeed CSV、SmokePing | 导出选定窗口 CSV/JSON | 可留证和外部分析 | commands/api/MonitorPage | 中 | P2 | 导出包含时间、接口、收发速率和缺失值 | 已完成 |
| EV-018 | 历史样本独立表 | 存储 | detail JSON 随记录增长，列表与样本耦合 | LibreSpeed 数据模型 | 新增 samples 表和 schema 版本，保留旧 detail 读取 | 大历史库更快 | storage.rs、迁移 | 高 | P2 | 旧记录可读，新记录样本分页读取 | 待实施 |
| EV-019 | 历史 FTS5 | 存储 | LIKE 搜索在大库上会变慢 | SQLite FTS5、Speedtest Tracker | 对节点/ISP/接口建立 FTS 虚表 | 大库搜索稳定 | storage.rs、迁移 | 中 | 10 万条 fixture 搜索有索引且结果一致 | 待实施 |
| EV-020 | 历史分页性能基准 | 性能 | 当前只有功能测试，无大数据基线 | SQLite 工程实践 | 生成 1k/10k/100k fixture 测列表/统计/导出 | 发现回归 | scripts/bench、docs | 中 | 记录 P50/P95 和内存，作为 CI 可选基线 | 待实施 |
| EV-021 | 独立稳定性测试页 | 测速/质量 | 质量阶段嵌在测速，无法单独长时间观察 | LibreSpeed stability test | 可配置时长、间隔、阈值、目标和 CSV | 适合断网/抖动排查 | engine/monitor/page | 高 | P1 | 运行 1/5/15 分钟并展示 min/max/avg/jitter/loss | 待实施 |
| EV-022 | 质量阈值区间告警 | 质量 | 当前告警偏向后台目标，缺少图表标线 | LibreSpeed | 图表显示阈值、连续超阈值计数和冷却状态 | 告警原因更直观 | QualityPage、scheduler | 中 | P2 | 模拟连续超阈值只通知一次并记录原因 | 待实施 |
| EV-023 | CLI 可重复实验元数据 | CLI | JSON 有结果但缺少完整设置快照 | iperf3、speedtest-cli | JSON 增加协议、并发、时长、节点 ID、版本 | 脚本结果可复现 | cli.rs、model.rs | 低 | P2 | 相同参数输出包含稳定字段，令牌不泄露 | 已完成 |
| EV-024 | CLI 历史查询 | CLI | CLI 只能测速，脚本无法读取历史 | Speedtest Tracker API | 增加 `history list/export` 只读命令 | 便于自动化取数 | cli.rs、storage.rs | 中 | P2 | 支持日期、状态、关键词和 JSON 输出 | 已完成 |
| EV-025 | 事件批量合并 | 性能 | 高频样本事件可能增加 WebView 重绘 | gping/Worker 模型 | 后端 100–200ms 聚合或前端 requestAnimationFrame 合并 | 长测更稳 | engine、store/test.ts | 中 | 采样不丢；UI 更新频率有上限 | 待实施 |
| EV-026 | 休眠/恢复状态 | 稳定性 | 休眠期间监控和计划测速行为缺少专门验证 | 桌面监控工具 | 监听系统时间跳变、暂停/恢复后台任务 | 避免错误长延迟和重复任务 | scheduler/monitor | 高 | 模拟时钟跳变不产生伪造样本或重复测速 | 待实施 |
| EV-027 | 网卡切换恢复 | 稳定性 | Wi-Fi/以太网切换后需观察监控和节点状态 | MySpeed health check | 网络状态事件触发监控重置和节点轻量重探测 | 网络切换后自动恢复 | monitor/selector | 中 | 切换接口后 30 秒内恢复新接口数据 | 待实施 |

## P2：产品能力、安全与工程化

| ID | 升级名称 | 模块 | 当前问题/依据 | 对标参考 | 实施方案 | 预期效果 | 文件/模块 | 难度 | 优先级 | 验收标准 | 状态 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| EV-028 | 多语言基础层 | 前端 | 文案主要硬编码中文，扩展成本高 | Tauri/React 社区惯例 | locale 文件、键名、语言设置、Rust 错误码 | 降低国际化成本 | locales、store、settings | 高 | P2 | 中文默认完整；英文覆盖导航、设置、错误和核心流程 | 待实施 |
| EV-029 | 屏幕阅读器验收 | 可访问性 | 目前有 ARIA 但缺真实读屏证据 | WAI-ARIA | Windows Narrator + 页面脚本验收 | 语义可用性可证明 | docs、所有页面 | 中 | P1 | 主要操作、状态、表格和弹窗可读 | 待实施 |
| EV-030 | 高 DPI 布局验收 | 桌面 UI | 仅源码与普通窗口截图验证 | Tauri Windows | 100/125/150/200% 缩放检查窗口、表格、弹窗 | 避免文字裁切 | tauri.conf、CSS、docs | 中 | P1 | 4 档缩放无重叠、按钮可用 | 待实施 |
| EV-031 | 依赖维护告警处置 | 安全 | cargo audit 当前有维护/unsound 告警 | cargo-audit | 逐项判断升级、替换或明确豁免边界 | 降低长期风险 | Cargo.lock、CI、docs | 中 | P1 | CI 保存审计清单；阻断漏洞与非阻断告警分开 | 待实施 |
| EV-032 | Windows Authenticode | 发布安全 | minisign 验证更新完整性，不证明发布者身份 | Windows 发布实践 | 证书、签名、时间戳、CI secret 和验证脚本 | 降低 SmartScreen 摩擦 | release.yml、docs | 高 | 安装包签名可由 `signtool verify` 验证 | 待实施 |
| EV-033 | 本地备份加密选项 | 隐私 | JSON/SQLite 本地文件可被同机用户读取 | 本地密码库实践 | 可选用户密码派生密钥，默认保持明文兼容 | 敏感历史可保护 | storage/settings | 高 | 开启后新备份无法直接读；忘记密码有明确恢复语义 | 待实施 |
| EV-034 | 诊断报告导出 | 诊断 | 诊断详情只能在页面或历史中查看 | Speedtest Tracker/LibreSpeed | 生成脱敏 HTML/JSON，明确未知和失败 | 便于工单共享 | commands.rs/api/DiagnosticsPage | 中 | P2 | 报告不含结构化敏感标识，失败项目保留原因 | 已完成 |
| EV-035 | 运行日志脱敏检查 | 安全 | 日志规则分散，新增适配器容易误记敏感值 | 本项目 NDT7 脱敏测试 | 增加日志字段扫描测试和禁止 token 模式 | 防止凭据进入日志 | Rust tests、CI | 中 | fixture 中 token/IP 按策略被删除或掩码 | 待实施 |
| EV-036 | 本地 IPC 契约测试 | 工程化 | 前后端类型和命令参数靠手工同步 | Tauri command layer | 从前端 invoke、Rust 注册表和命令定义生成/校验契约 | 早发现命令漂移 | scripts、package.json、README | 中 | CI 检查命令名、注册和定义一致 | 已完成 |
| EV-037 | Windows UI 冒烟 CI | 工程化 | 目前 CI 不启动真实窗口 | Tauri driver / WebDriver | Windows runner 启动隔离应用，遍历 8 页并取消测速 | 回归真实用户路径 | `.github/workflows/e2e.yml` | 高 | 启动、切页、开始/取消、导出路径通过 | 待实施 |
| EV-038 | 安装器回滚测试 | 发布 | 只有构建证据，没有失败回滚证据 | Tauri NSIS | 隔离目录安装旧版/新版，模拟下载和重启失败 | 发布恢复更可靠 | release scripts/docs | 高 | 失败不破坏旧版本，日志明确 | 待实施 |
| EV-039 | 节点清单自动刷新 | 运维 | fallback JSON 是构建时快照 | 当前 ROADMAP 技术债 | CI 定时探测、生成 PR，保留人工审核 | 失效节点更少 | scripts/workflow | 中 | 仅 HTTPS、真实 200 响应、变更可审查 | 待实施 |
| EV-040 | 插件扩展边界设计 | 架构 | provider 和诊断目标目前编译期扩展 | Tauri/Netronome 生态 | 先定义受限 manifest/能力模型，不执行任意代码 | 后续扩展可控 | docs/engine adapter | 高 | 文档明确权限、签名、生命周期和回滚 | 待实施 |
| EV-041 | 远端分布式拨测 | 产品 | 单机批量拨测不等同于 ITDOG 多点 | ITDOG、Netronome | 设计可选受信探针协议和结果签名 | 观察地域差异 | future service/protocol | 高 | 探针身份、延迟、失败和隐私可验证 | 待实施 |
| EV-042 | 端到端加密云同步 | 产品/隐私 | 当前完全本地，跨设备不可用 | MySpeed 自托管、云同步产品 | 先做端到端密钥和冲突模型，再决定服务端 | 可选跨设备历史 | future sync | 高 | 默认关闭；服务器看不到明文；可离线恢复 | 待实施 |

## 执行顺序

1. 先完成 EV-015 的更新安装 E2E，并把 EV-016 的当前窗口聚合扩展为持久化 24 小时/7 天趋势。
2. 再完成 EV-018～EV-027 的历史性能、稳定性测试和网络切换恢复。
3. 最后推进 EV-028～EV-042 的多语言、生态和分布式能力。

本轮已完成 EV-001～EV-014、EV-017、EV-023、EV-024、EV-034、EV-036；EV-016 为部分完成；其余项目保留为可独立验收的后续工作，没有把规划项标记为完成。
