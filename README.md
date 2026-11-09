# NetVision Pro

**专业级 Windows 桌面网络测速与诊断工具**

NetVision Pro 面向需要了解真实网络质量的个人用户、开发者和运维人员，提供可解释的测速结果、网络诊断、历史分析与实时监控。所有应用内测量都由本机 Rust 引擎真实完成，不使用模拟数据。

[下载最新安装包](https://github.com/yuchen0x1/Internet-Speed-Test/releases/latest) · [查看全部 Releases](https://github.com/yuchen0x1/Internet-Speed-Test/releases)

## 功能

### 真实测速

- 多流并发测速：延迟、下载、上传、质量分析完整分阶段执行
- 支持 LibreSpeed、Cloudflare Speed 和 M-Lab NDT7
- 自动从在线列表、内置兜底和 Cloudflare 边缘节点中选择合适节点，也支持自定义 HTTPS 节点
- 实时速度曲线、阶段事件链、累计流量和可安全取消的测速过程
- 质量评分综合延迟、抖动、Bufferbloat、ICMP 丢包和 HTTP 失败，并给出可解释结论

### 诊断与监控

- 网卡、网关、DNS、TCP、HTTPS、IPv6、ICMP、路由追踪和 NAT 诊断
- 多目标批量 ICMP 拨测、延迟聚合、单目标走势和 DNS 解析分布
- 网卡真实收发速率监控：1 秒采样、1 小时环形缓冲和多时间窗口
- 服务连通性矩阵、连续 Ping 监测和网络质量雷达

### 历史与自动化

- SQLite 本地历史库：筛选、排序、对比、趋势图和历史详情回放
- CSV、JSON、HTML 报告导出，支持历史保留策略
- 计划测速、断网/恢复通知、托盘操作和本地 Prometheus/Grafana 指标
- CLI 与桌面端复用同一测速引擎，可输出完整 JSON 结果

### 官方测速入口

应用内提供固定白名单的国内外官方测速站入口，包括 SpeedTest.cn、运营商测速入口、Ookla Speedtest、FAST.com 和 Cloudflare Speed。入口会交给系统浏览器打开，第三方网站的结果不会写入 NetVision Pro 历史，也不会被应用抓取或代跑。

## 下载与安装

- **系统**：Windows 10/11 x64
- **依赖**：WebView2（Windows 10/11 通常已自带）
- **安装方式**：下载 Release 中的 `NetVision.Pro_1.8.0_x64-setup.exe`，双击安装即可。默认按当前用户安装，不需要管理员权限。

安装包由 Tauri NSIS 生成，最新版本可在 [Releases](https://github.com/yuchen0x1/Internet-Speed-Test/releases) 页面获取。

## CLI

安装后可以在应用目录中运行 CLI，与桌面端共享测速引擎和历史库：

```powershell
.\netvision-pro.exe cli speedtest --json --duration 10 --down 6 --up 4
```

可选参数：

```text
--node <url>     手动指定 HTTPS 测速节点
--precision      使用更高精度的延迟采样
--no-save        不写入本地历史
```

进度输出到 `stderr`，结果输出到 `stdout`；`--json` 返回完整 JSON，包括实时样本和质量评级。退出码为：`0` 完成、`1` 参数错误、`2` 已取消、`3` 测速失败。

## 测量口径

1. 检查网络接口，获取在线节点并合并内置兜底列表；对候选节点进行并发延迟和下载能力探测后自动选择。
2. 延迟使用多次 HTTP 往返测量，报告中位数、最小值、最大值和抖动。
3. 下载和上传均使用真实多流传输与流式字节计数；下载会剔除 TCP 慢启动预热期，上传按自适应分块持续发送随机数据。
4. 质量阶段并行测量负载延迟（Bufferbloat）和目标主机 ICMP 丢包；HTTP 请求失败与网络丢包分别统计。
5. 所有速度内部统一按字节/秒计算，`Mbps = bytes × 8 / 1e6`。网卡协商速率只作系统信息展示，不会被当作宽带速度。

## 技术实现

- **桌面框架**：Rust + Tauri 2 + Windows WebView2
- **前端**：React 19、TypeScript、Zustand、Canvas 图表
- **测速协议**：LibreSpeed HTTP 多流、Cloudflare Speed、M-Lab NDT7 WebSocket
- **存储**：SQLite WAL、本地 JSON 设置、HTML/CSV/JSON 导出
- **系统能力**：`sysinfo` 网卡流量、`netdev` 接口信息、Windows ICMP API、路由和 DNS 探测

核心代码按测速引擎、诊断、监控、存储、设置、报告和 Tauri 命令层分层；前端通过命令层访问本地能力，网络 I/O 不直接在前端执行。

## 开发与构建

开发环境：Node.js 18+、Rust 1.80+（MSVC 工具链）、Windows WebView2。

```bash
npm install
npm run icons        # 首次构建或图标变更后执行
npm run dev          # 启动前端开发服务
npm run tauri dev    # 启动桌面开发版
npm run typecheck    # TypeScript 类型检查
npm run test         # 前端单元测试
cargo test --locked --manifest-path src-tauri/Cargo.toml
npm run tauri build  # 生成 Windows NSIS 安装包
```

安装包输出目录：

```text
src-tauri/target/release/bundle/nsis/
```

构建前建议先运行 `npm run typecheck`、`npm run test` 和后端测试。真实网络集成用例默认标记为 `#[ignore]`，需要时显式执行：

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml -- --ignored
```

## 隐私、安全与已知限制

- 设置、历史和诊断数据默认只保存在本机；卸载不会自动删除历史数据。
- 公网 IP/ISP 只在用户主动开启相关设置后查询，并且仅使用测速同源服务。
- NDT7 优先使用 WSS；Locate 访问令牌只存在于 Rust 适配器运行时内存，不写入前端状态、CLI JSON 或历史记录。NDT7 延迟是 WebSocket 握手时延近似值，不等同于 ICMP/HTTP Ping。
- 外部测速站只允许固定服务 ID 和固定 HTTPS 地址；应用不接受任意 URL，不抓取网页，不上传测速数据。
- 诊断结果会受目标服务器策略、运营商线路、Wi-Fi 信号和本地防火墙影响。目标禁用 ICMP 时显示未知，不会伪造为 0% 丢包。
- 内置节点以国际节点为主；受限网络建议在节点页添加可访问的本地 LibreSpeed HTTPS 节点。

## v1.8.0

- 接入 M-Lab NDT7：通过 Locate API v2 动态发现节点，支持 WSS/WS 和真实 WebSocket 下载、上传。
- 完善 NDT7 令牌与端点边界：敏感信息不进入 IPC、事件、CLI JSON 或历史存储。
- 优化节点选择、测速过程展示和质量分析，并保留 LibreSpeed、Cloudflare Speed 与自定义节点能力。

## 许可证

[MIT License](LICENSE)

部分动效与可视化模式复用了 [Caho1/open-claude-ui](https://github.com/Caho1/open-claude-ui) 的 MIT 许可实现，详见源文件说明。
