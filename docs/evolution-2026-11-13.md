# NetVision Pro 自主升级报告 · 2026-11-13

> 本文是发布前开发阶段的历史记录；正式版本信息见 [发布说明](releases/release-notes.md)。

本轮仅修改当前仓库，基线为 `main@97cb378`。开始时已有 12 个已跟踪文件修改及 `.evolution/`、自动更新说明和截图等未跟踪文件；均保留，未重置、清理、提交、推送或发布。版本号仍为 1.11.0，新增内容归入 Unreleased。

## 项目认知与验收清单

- [x] 核实项目边界、Git 状态、README、配置、依赖、CI 与重要源代码。
- [x] 沿 UI → Zustand → Tauri commands → Rust 引擎/SQLite 阅读真实调用路径。
- [x] 联网研究五个相关开源项目的源码、目录、许可证及最近提交。
- [x] 选择并实现高价值修复，保持现有协议、数据格式和功能入口。
- [x] 补充日期、全量统计、趋势、异步竞态、导出和终态回归测试。
- [x] 同步 README、使用指南、Changelog 和本报告。
- [x] 完成隔离 Windows 桌面运行、真实测速/取消、历史自动刷新和截图验收。

产品定位：Windows 10/11 x64 网络测速与诊断桌面工具。前端为 React 19、TypeScript、Zustand、Vite，桌面框架为 Tauri 2，后端为 Rust/Tokio/reqwest，历史使用 SQLite WAL。没有独立远端业务后端或账号数据库。Node/Rust 依赖已安装，无需更换框架或添加生产依赖。

| 模块 | 实际功能与入口 | 数据/架构 |
| --- | --- | --- |
| 测速/网络质量 | `SpeedTestPage`、`QualityPage`；LibreSpeed HTTP、Cloudflare HTTP、M-Lab NDT7 WebSocket；多流、取消、负载延迟、ICMP、评级 | `engine/mod.rs` 编排，协议适配器独立，事件同步到 `store/test.ts` |
| 节点 | 优选、探测、收藏、自定义节点、手动/自动模式 | `engine/selector.rs`；收藏存 SQLite，自定义节点存设置 |
| 诊断/批量拨测 | 网卡、DNS、网关、TCP/HTTPS、IPv6、NAT、路由、ICMP、多目标拨测 | Rust `diagnostics`、`batchtest`；诊断历史为独立表 |
| 监控 | 网卡收发、环形历史、暂停/恢复、连续 Ping | `monitor.rs`、`pingmon.rs`，前端事件订阅 |
| 测速历史 | 游标分页、筛选、比较、趋势、热力图、详情、CSV/JSON/HTML | `storage.rs`、`commands.rs`、`report.rs` |
| 自动化/集成 | 计划测速、阈值提醒、托盘、CLI、仅本机的 Prometheus 端点 | 调度器复用引擎；CLI 复用相同协议实现 |
| 设置/更新 | JSON 设置、主题、隐私、开机启动、签名更新入口 | 已有未提交更新检查改进；本轮未运行安装/更新或修改发布配置 |

启动：`npm run dev` / `npm run tauri dev`；前端构建：`npm run build`；桌面构建：`npm run tauri -- build --no-bundle`。测试脚本为 Vitest 与 Cargo，CI 另有 rustfmt、Clippy、依赖审计。生产目录扫描未发现显式 TODO/FIXME/todo!/unimplemented! 标记；这不等同于所有分支均已验收。测试 fixture 不代表生产使用模拟数据。

## 竞品研究与许可证

2026-11-13 经 GitHub 连接器读取源代码和目录，并检查最新提交。下面日期为当时 API 返回的最新提交日期，不能代替后续维护保证。实现为独立编写，未移植任何竞品代码；无需引入 LGPL 或 Apache 代码的分发义务。

| 项目 | 实际阅读依据 | 许可证 / 最近提交 | 关联与取舍 |
| --- | --- | --- | --- |
| [LibreSpeed](https://github.com/librespeed/speedtest) | [worker 状态机、取消与采样配置](https://github.com/librespeed/speedtest/blob/master/speedtest_worker.js)、[协议说明](https://github.com/librespeed/speedtest/blob/master/doc.md) | [LGPL-3.0](https://github.com/librespeed/speedtest/blob/master/LICENSE)；2026-11-12 | 借鉴终态/取消边界与工作线程分离的原则；本项目已支持该协议，本轮只修复事件终态与计时，不重复接入 |
| [Speedtest Tracker](https://github.com/alexjustesen/speedtest-tracker) | [Result 模型](https://github.com/alexjustesen/speedtest-tracker/blob/1.x/app/Models/Result.php)、[图表范围筛选](https://github.com/alexjustesen/speedtest-tracker/blob/1.x/app/Filament/Widgets/Concerns/HasChartFilters.php)、目录中的导出和图表模块 | [MIT](https://github.com/alexjustesen/speedtest-tracker/blob/1.x/LICENSE.md)；2026-10-26 | 采用日期快捷范围和明确统计范围的思路；保持本机 SQLite，不引入其服务端账号体系 |
| [OpenSpeedTest](https://github.com/openspeedtest/Speed-Test) | [README](https://github.com/openspeedtest/Speed-Test/blob/main/README.md)、[界面/状态实现](https://github.com/openspeedtest/Speed-Test/blob/main/assets/js/app-2.5.4.js) | [MIT](https://github.com/openspeedtest/Speed-Test/blob/main/License.md)；2026-04-22 | 研究轻量实现和清晰过程反馈；保留当前桌面仪表盘，不移植 SVG 页面或压力测试入口 |
| [Cloudflare Speedtest](https://github.com/cloudflare/speedtest) | [带宽引擎、TTFB/传输计量、重试](https://github.com/cloudflare/speedtest/blob/main/src/engines/BandwidthEngine/BandwidthEngine.ts)、`src/Results` 与协议引擎目录 | [MIT](https://github.com/cloudflare/speedtest/blob/main/LICENSE)；2026-11-09 | 保持测量状态、错误和未知值的区分；不照搬浏览器 Resource Timing 算法到 Rust，不修改已实现的字节计数方式 |
| [M-Lab ndt7-client-go](https://github.com/m-lab/ndt7-client-go) | [客户端、Locate、上下文与握手超时](https://github.com/m-lab/ndt7-client-go/blob/main/ndt7.go)、命令行/导出器目录 | [Apache-2.0](https://github.com/m-lab/ndt7-client-go/blob/main/LICENSE)；2026-02-20 | 用于核对协议与 CLI 分层方向；已有 Rust NDT7，不额外引入 Go 客户端或新协议依赖 |

| 维度 | 本轮之前的实际不足 | 参考实践 | 本轮成果 |
| --- | --- | --- | --- |
| 核心功能 | 结束日期按裸字符串比较，漏掉该天带时分秒的记录 | 范围筛选应与历史时间契约一致 | 后端统一验证并规范日期，保留旧时间戳调用的精确边界 |
| UI | 日期、排序、导出挤在标题栏，缺少导出状态 | 专注任务的操作分组与过程反馈 | 独立工具栏、明确日期标签、快捷范围、禁用/忙碌反馈 |
| 用户体验 | 导出忽略筛选；趋势随列表排序反向 | 历史分析与导出采用同一用户范围 | 贯通筛选/排序，趋势独立按时间排序，错误可重试 |
| 数据架构 | 统计仅基于已加载的一页；上传未知被当成零 | 数据状态与分析口径清晰 | SQL 聚合全筛选范围，完成/失败/取消分计，AVG 跳过 NULL |
| 自动化 | finished 事件先发出，历史写库稍后才执行 | 生命周期完成应意味着结果可被下游读取 | 写库结束后发布完整终态，计划/手动测速共用修复 |
| 扩展 | UI 直接散布部分日期/趋势逻辑 | 协议、模型、展示分层 | 独立历史纯函数、类型化 summary IPC、回归测试；旧 API 保持可用 |

## 分级发现与实现

| 优先级 | 问题/机会 | 收益、风险与决策 |
| --- | --- | --- |
| P0 已修复 | 结束日期漏数据；完成事件与数据库竞态 | 避免错误筛选和结果看似丢失；无库结构迁移，风险较低，增加边界与实际运行验收 |
| P1 已完成 | 全范围统计、筛选导出、未知上传口径 | 直接改善长期测速记录的可信度；使用已有 SQLite/IPC，不增加基础设施 |
| P1 已修复 | 失败/取消状态与完整结果不一致，写库失败仅写日志 | 完整结果供历史/状态栏刷新；测速状态和保存状态分开，旧记录通过 serde 默认字段兼容 |
| P2 已完成 | 快捷日期、重试/刷新、请求乱序、趋势、计时和采样上限 | 减少操作并修复误导性展示；沿用设计变量、保留键盘原生控件，无额外生产依赖 |
| P2 后续 | 全范围日均/趋势聚合、后台长时监控会话管理 | 当前图表仍只取已加载记录且已明确标注；大数据可用后端分桶聚合进一步优化 |
| P1 后续复查已修复 | 设置落盘和 OS 开机启动副作用发生失败时的一致性 | 补偿逻辑与隔离故障用例见 [二次复查](recheck-2026-11-13.md)；真实 OS 故障注入仍未覆盖 |
| P3 后续 | 协议一致的重复测速基准、多网卡/时段对比、测量流量预算 | 需要明确运营商/计费场景和代表性方法；不强加 AI、插件系统或新服务 |

安全方面：继续保留固定外部站点白名单、后端主机校验、SQL 参数绑定、CSV 公式中和、NDT7 令牌不序列化、Prometheus 仅回环监听及用户授权后获取公网信息。新增筛选校验覆盖非法/反向日期。上述为相关源代码审阅和已有回归覆盖，不能等同全面渗透测试。npm audit 在线查询失败，不能承诺当前依赖零漏洞。

## 重要文件变化

| 文件 | 原因 |
| --- | --- |
| `src-tauri/src/storage.rs` | 一致日期边界、全范围 SQL summary、日期/分页统计用例 |
| `src-tauri/src/commands.rs`、`lib.rs` | 新增 summary 命令，CSV/JSON 接受可选筛选/排序 |
| `src-tauri/src/engine/mod.rs`、`model.rs`、`cli.rs` | 所有结果写库后发布，保存警告独立字段与 CLI stderr 提示 |
| `src/api.ts`、`types.ts` | 类型与 IPC 契约同步，原无筛选调用继续可用 |
| `src/pages/HistoryPage.tsx`、`src/styles/pages.css`、`src/store/ui.ts` | 全范围卡片、工具栏、状态和范围切换、请求过期保护 |
| `src/lib/history.ts` | 本地日历范围与按实际时间排序的趋势纯函数 |
| `src/store/test.ts`、`src/pages/SpeedTestPage.tsx` | 连续计时、有界单调采样、真实失败/取消展示、持久保存警告 |
| `src/pages/HistoryPage.test.tsx`、`src/lib/history.test.ts`、`src/store/test.test.ts`、`src/api.test.ts` | DOM 交互、异步竞态、状态与接口回归 |
| `README.md`、`CHANGELOG.md`、`docs/usage.md`、本文 | 真实能力、未发布状态、使用方法和验证边界 |

未删除文件、重建数据库或改变表结构。新增结果字段采用默认值，历史 JSON 可继续读取。测试中使用内存数据库和明确标注的 IPC fixture；它们不进入正式界面。

## 测试证据

初始基线：前端 37 PASS，Rust 59 PASS/3 ignored，类型检查 PASS。新增 DOM 测试首次因未声明 jsdom 环境失败，补充测试文件环境声明后通过；没有弱化断言。

| 检查 | 结果 | 证据与边界 |
| --- | --- | --- |
| `npm run test` | PASS | 7 文件、45 用例；含 DOM 交互、过期响应、游标分页去重、导出 IPC、计时与保存告警 |
| `npm run typecheck` | PASS | TypeScript 无错误 |
| `npm run build` | PASS | 生产前端构建成功；主 JS 约 298 KB，历史页独立 chunk 约 18.8 KB |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml` | PASS | 62 通过，3 个原有真实网络用例仍为 ignored；本轮另行执行下述原生网络测试 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | PASS | 格式检查无错误 |
| `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | PASS | 严格 lint 无错误 |
| `cargo build --locked --manifest-path src-tauri/Cargo.toml` | PASS | Windows 开发可执行文件构建成功 |
| `npm run tauri -- build --no-bundle` | PASS | 原生 release 优化构建成功；未打包、安装或签名发布 |
| 隔离原生 UI / IPC | PASS | 使用 Tauri `appDirectoriesOverride`、独立 identifier 和 WebView 数据目录，启动后再次核实 `get_app_info.data_dir` 位于仓库 `src-tauri/target/evolution-qa/` |
| 日期快捷范围、深浅主题布局 | PASS | 原生 WebView 的 1380×850 深色与 1060×680 浅色；日期控件可见、无横向溢出，人工查看截图 |
| 真实 Cloudflare 测速 → SQLite → 历史自动刷新 | PASS | 5 秒/单流真实上传下载，下载 24 个采样、上传 26 个采样；历史页未点击刷新即显示新记录，详情读回状态 completed |
| 原生取消与 CLI 失败 | PASS | 立即取消后入库 cancelled；CLI 访问不可用回环端口，捕获 JSON status=failed、错误字段与退出码 3。只读查询隔离数据库确认 completed/cancelled/failed 各一条 |
| 导出 | PARTIAL | DOM/IPC 用例验证筛选与排序，后端列表/计数验证同一日边界；未自动操作原生保存对话框，也未做超 5000 条真实文件验收 |
| `cargo audit --db src-tauri/target/evolution-advisory-db --file src-tauri/Cargo.lock` | PASS（有警告） | 更新 RustSec 数据库后检查 565 个锁定依赖，退出码 0；保留 5 项警告，见下文 |
| `npm audit --audit-level=high --registry=https://registry.npmjs.org` | BLOCKED | npm 官方漏洞接口连接 ECONNRESET，不能据此判断当前 npm 依赖无漏洞 |
| 安装/跨版本更新、休眠恢复、磁盘满故障、长期内存/大库性能 | PARTIAL（这些项目未执行） | 本轮未运行安装器或更新器，没有相应验收证据；系统设置故障注入会超出隔离数据测试边界 |

Rust 审计保留的警告：`proc-macro-error`、`zbus_names`、`zvariant`、`zvariant_derive` 的 unmaintained，以及 `glib` 的 unsound。退出码 0 仅表示本次审计未触发默认漏洞失败门禁，不表示这些警告已修复。

已解决的测试环境问题：内置浏览器运行时缺少模块；Python Playwright 安装因 TLS EOF 失败；隔离安装 Node Playwright 因 npm ECONNRESET 失败。最终使用 Node 内置 WebSocket 连接本次隔离应用的 WebView2 调试端口完成实际交互，没有添加生产依赖或模拟 IPC。初次 PowerShell CLI 检查没有正确等待 GUI 子系统可执行文件结束，改为等待子进程并捕获管道后通过。没有关闭 TLS 校验、修改全局代理或操作真实应用数据库。

验收时的本地辅助配置、脚本和测试库位于被 Git 忽略的 `src-tauri/target/evolution-qa/`，不用于发布。2026-11-13 清理时，临时脚本、截图、结果文件与 QA 可执行文件已移入本机回收站，隔离测试库及配置仍保留。截图来自 QA 构建，使用真实成功与取消记录；最终前端另将统计卡标题从“总测试”明确为“成功测试”。截图不是安装包、签名或线上发布证据。

原生深浅色实测截图保存在本地；按照仓库贡献规范，不提交本机测速截图和原始网络数据。

## 完成度评价

评分为本轮源码和本地验证所支持的工程成熟度判断，满分 10 分，不是性能基准或安全认证。

| 维度 | 评分 | 依据与边界 |
| --- | --- | --- |
| 功能完整度 | 8 | 测速/诊断/历史/监控链路完整；长期会话与全量图表待扩展 |
| UI/UX | 7 | 操作分组、日期口径、错误/忙碌态改善；已验证两种 Windows 原生视口，尚未覆盖系统缩放和屏幕阅读器 |
| 代码质量 | 8 | 模型/协议/存储分层，类型与严格 Clippy；平台副作用缺少故障注入 |
| 性能 | 7 | 有界采样、游标分页、SQL 聚合；未做长时内存、超大库/不同带宽性能基准 |
| 安全性 | 6 | 沿用输入校验、隐私和令牌隔离；本轮在线依赖审计受阻，非渗透测试 |
| 稳定性 | 7 | 修复事件/存储竞态与异步 UI 乱序；尚未验证系统重启、休眠和跨版本升级 |
| 可维护性 | 8 | 无新增生产依赖，兼容旧数据/调用，关键逻辑有定向回归 |
| 扩展性 | 7 | 类型化 summary 接口与协议适配器可扩展；无需求证据时不引入插件平台 |

本轮目标是交付可审阅的本地升级。远端 CI、签名安装包、真实自动更新安装链路和生产发布均不在本次完成声明中。
