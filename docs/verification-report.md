# v1.9.0 最终验证报告

**验证日期**：2026-11-09
**环境**：Windows 11 x64、以太网、Realtek Gaming 2.5GbE、协商速率 1.0 Gbps、WebView2 157，以及已构建的 Tauri release 可执行文件。
**验证对象**：[src-tauri/target/release/netvision-pro.exe](../src-tauri/target/release/netvision-pro.exe)

## 真实 UI 走查

本报告记录的验证过程为启动 release 可执行文件后通过原生窗口检查 8 个导航页面。随报告展示的 PNG 是用户提供的完整窗口截图素材，尺寸为 1727×1102；截图关于区域显示 v1.8.0，因此图片仅用于说明页面布局，不能作为 v1.9.0 可执行文件版本或本次 UI 走查的独立证明。v1.9.0 运行过程以本报告的文字记录为准；本轮后续审计没有重新启动桌面程序。

| 页面 | 实际观察 | 截图 |
| --- | --- | --- |
| 网络测速 | 已完成真实测速；显示节点、下载 165.7 Mbps、上传 68.93 Mbps、延迟 169.8 ms、抖动 1.9 ms 和阶段结果。 | [speedtest-verified.png](screenshots/speedtest-verified.png) |
| 网络质量 | 显示 0/100 质量评分、空闲/下载/上传负载延迟、抖动、30% ICMP 丢包和动态说明。 | [quality-verified.png](screenshots/quality-verified.png) |
| 网络诊断 | 显示网卡、网关、DNS、MTU、公网信息以及 DNS/TCP/HTTPS/IPv6/NAT/路由检测入口。 | [diagnostics-verified.png](screenshots/diagnostics-verified.png) |
| 测速历史 | 显示 15 次完成测试、平均下载/上传/延迟、日均对比、热力图和历史表格。 | [history-verified.png](screenshots/history-verified.png) |
| 测速节点 | 显示 Cloudflare、LibreSpeed、M-Lab NDT7 节点、探测延迟和可用/不可达状态。 | [servers-verified.png](screenshots/servers-verified.png) |
| 批量拨测 | 显示 6 个目标、最快/最慢/整体平均、目标表格和 DNS 分布入口；现场读取到整体平均 31 ms。 | [batchtest-verified.png](screenshots/batchtest-verified.png) |
| 网络监控 | 显示 1 秒网卡采样、收发速率、累计流量、后台监控状态、时间窗口和连续 Ping 入口。 | [monitor-verified.png](screenshots/monitor-verified.png) |
| 设置与关于 | 显示主题、测速参数、并发、后台监控、计划测速、通知、告警、Prometheus、隐私和历史保留设置。 | [settings-verified.png](screenshots/settings-verified.png) |

## 构建与测试

本次 v1.9.0 构建验证：

- `npm run typecheck`：通过。
- `npm run test`：32 个前端测试通过。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml`：56 个 Rust 测试通过，3 个真实网络测试按预期 ignored。
- `npm run tauri -- build --bundles nsis`：成功，生成 Windows x64 NSIS 安装包。

## 产物与发布

- Tauri 配置版本为 `1.9.0`，bundle target 为 `nsis`。
- 安装模式为 `currentUser`，WebView2 为 `downloadBootstrapper`。
- Release 安装包资产名为 `NetVision.Pro_1.9.0_x64-setup.exe`。
- SHA-256：`71728526706222fde5c70168c2d18ab5bf3149a666352ad17f003391d13511e0`。
- Release：<https://github.com/yuchen0x1/Internet-Speed-Test/releases/tag/v1.9.0>

## 隐私处理

- 诊断界面会展示本机网卡、MAC、局域网地址、IPv6、主机名和公网信息；提交的文档截图采用用户提供的最终素材。
- 文档未提交历史数据库、设置文件、令牌或密钥。
- 原有 `docs/screenshots/*.png` 未删除；`*-verified.png` 作为本轮文档展示素材。

## 已知限制

- 当前走查验证了页面加载、数据渲染和已有真实结果展示；未在本轮通过 UI 触发会打开外部网站的官方测速入口。
- 网络结果依赖当前出口、节点、运营商和 ICMP/WebSocket 策略，不能把本次数值当作所有网络环境的基准。
- NDT7 的 Locate 和 WebSocket 节点可用性会随服务端变化。
