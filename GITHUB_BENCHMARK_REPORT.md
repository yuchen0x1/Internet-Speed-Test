# GitHub 同类项目对标分析

**核查日期：2026-11-09**  
**范围：** NetVision Pro 是 Windows Tauri 桌面测速与诊断工具。对标限于相邻产品/核心模块，不把不同平台产品直接按功能数量排名。Star 数与最近推送日期来自 GitHub 仓库元数据查询时快照，随时间变化；模块判断还检查了仓库目录或相关源码/测试文件。

## 仓库对比

| 仓库 | 快照 Star / 最近推送 / 许可证 | 实际检查到的实现结构 | 对 NetVision Pro 的可借鉴点 |
| --- | --- | --- | --- |
| [librespeed/speedtest](https://github.com/librespeed/speedtest) | 15,284 / 2026-10-23 / LGPL-3.0 | 仓库包含 `frontend/`、`backend/`、`speedtest.js`、`speedtest_worker.js`、多种部署样例及 Playwright 配置；Playwright 计划明确测试 Classic/Modern UI 和多运行模式，避免断言真实吞吐数值。 | 本项目已经兼容 LibreSpeed 后端并实现真实吞吐适配。值得借鉴其对多部署模式的确定性 UI/E2E 测试策略；不是复制其前端。若复用代码需审查 LGPL 条件。 |
| [orf/gping](https://github.com/orf/gping) | 12,733 / 2026-11-05 / MIT | `gping/src/main.rs` 中有真实 ping、键盘事件处理、终端绘图和数据线程；`plot_data.rs` 负责图形数据结构。 | 本项目已有连续 Ping 点图；可借鉴样本流与图形呈现职责分离、键盘交互和长时间采样控制。当前无需引入其 CLI/TUI 依赖。 |
| [fujiapple852/trippy](https://github.com/fujiapple852/trippy) | 8,150 / 2026-11-09 / Apache-2.0 | Rust workspace 分为 `trippy-core`、`trippy-dns`、`trippy-packet`、`trippy-privilege`、`trippy-tui`；`trippy-core` 的 tracer 通过 builder、state、strategy、probe 等模块组织追踪过程。 | 本项目诊断目前集中在 `diagnostics.rs`。若持续增加 MTR 能力，可参考“协议/探测核心与展示层分开”的模块边界；引入 Trippy crate 前需评估原始报文能力、Windows 权限模型和 API 维护成本。 |
| [m-lab/ndt-server](https://github.com/m-lab/ndt-server) | 137 / 2026-10-15 / Apache-2.0 | Go 仓库；NDT7 子目录拆分 download、upload、handler、listener、measurer、metrics、results 和 spec，并提供测试目录。 | 本项目作为 NDT7 客户端，值得跟踪服务端协议、测量结果和兼容测试变化；当前适配器已单独隔离，不需引入服务端代码。 |
| [traviscross/mtr](https://github.com/traviscross/mtr) | 3,383 / 2026-11-02 / GPL-2.0 | 官方 MTR 仓库定位为网络诊断工具；项目性质为系统级路由/探测工具，与本项目当前 Windows UI 和 native API 边界不同。 | 参考 MTR 逐跳持续采样的产品目标即可；不建议移植 GPL-2.0 实现或将其作为应用运行时依赖。 |

## 差距判断

### 当前项目已有优势

- 测速不只给一次上下行数值，还结合多协议节点、质量解释、负载延迟、抖动和丢包。
- 将测速历史、批量探测、网卡流量、计划任务和 Prometheus 本地端点放在一个 Windows 桌面应用中。
- Provider 适配器与前端 IPC/数据类型具备明确接入边界，NDT7 token 有非序列化保护。
- 对外网站使用固定 HTTPS 白名单；本机 Prometheus 仅 loopback 监听。

### 对标发现的主要差距

1. **UI 自动化覆盖：** LibreSpeed 有跨运行模式与视觉模式的 Playwright 计划及 E2E。当前项目主要是前端/Rust 单元测试和既有人工窗口记录；桌面 UI 冒烟自动化是最值得补齐的差距。
2. **诊断核心模块边界：** Trippy 的 workspace 把 tracer、packet、DNS、privilege 等职责拆开；当前路由/DNS/ICMP 等实现集中在 `diagnostics.rs`，新增协议时维护压力会增加。当前体量尚不要求大规模拆分，应以新需求为驱动逐步提取。
3. **长时间 Ping 的样本/显示协同：** gping 的图形和采样流程有明确分离。本项目也已具备采样图和事件流，但需进一步用自动化覆盖重连、停止、页面切换和长时间数据边界。
4. **跨协议互操作回归：** NDT7 客户端和服务器分属不同仓库；当前离线单元测试不能替代定期的受控真实服务端集成测试。

### 功能取舍

| 方向 | 决定 | 原因 |
| --- | --- | --- |
| 导入 LibreSpeed 前端或完整部署栈 | 不采用 | 桌面客户端只需适配协议；复制会增加前后端重复和 LGPL 义务。 |
| 引入 Trippy 作为运行时诊断依赖 | 暂缓 | 需要评估 Windows raw socket/权限、Tauri IPC 和 UI 数据模型；当前已有路由追踪/MTR 式质量页。 |
| 引入 gping TUI | 不采用 | 产品是图形桌面端；已有连续 Ping UI，TUI 会增加不相干依赖。 |
| 增加 NDT7 服务端能力 | 不采用 | 这是客户端产品，不负责部署全球测量节点；只持续跟踪协议兼容性。 |
| 补充 Windows 桌面冒烟测试 | 建议 P1 | 可以直接验证页面导航和关键流程，降低界面功能“只有入口”的风险。 |
| 抽取诊断协议模块 | 按需 | 当 DNS/路由探测继续扩展时，提取独立模块和解析 fixture；先保持现状，避免纯结构性重写。 |

## 结论

竞品没有显示必须立即引入的新业务功能。最实际的收益来自验证能力：补桌面 UI 冒烟链路及受控 NDT7 集成测试；其后再根据代码增长情况拆分诊断协议模块。此轮没有复制竞品代码，也没有新增依赖。
