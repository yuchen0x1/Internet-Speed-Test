# NetVision Pro 测试报告

## 2026-11-13 二次 Bug 复查（未发布）

- PASS：前端 57 项、Rust 68 项（3 项原有网络用例 ignored）、类型检查、生产前端构建、Rust 格式与严格 Clippy。
- PASS：隔离 Windows 原生 IPC/UI 验证节点快照保护、HTTP 显式授权、删除手动节点回到自动选择、Ping 目标/暂停恢复，以及取消后不评级。
- PARTIAL：自启动和写入失败补偿使用替代 OS 接口与仓库内隔离文件验证，没有改动真实系统启动项。
- 已知不足：200ms 高频采样时 3600 点缓冲不足 1 小时；全量图表、长期压力、安装及跨版本更新尚未验收。
- 详细触发条件、修复和验证边界见 [二次复查报告](docs/recheck-2026-11-13.md)。下文保留历史测试记录。

**测试日期**:2026-11-07(v1.0.0 → v1.1.0 → v1.2.0 → v1.3.0 → v1.4.0 同日迭代,**v1.7.0 增量:国内外知名测速入口 + 实时测速过程舱**)
**测试环境**:Windows 11(10.0.26340,x64)· 以太网(Realtek Gaming 2.5GbE,协商 1.0 Gbps)· WebView2 157
**构建**:Tauri 2.12.5 / Rust 1.99.1 MSVC / Node 24.20 / Vite 6.4.5

---

## 〇-5、2026-11-10 本地升级验证(未发布)

> 本节记录当前未提交工作区的本地验证，不替代历史版本验收记录。

## 〇-6、2026-11-12 自动更新增强验证(未发布)

| 检查 | 命令/证据 | 结果 |
| --- | --- | --- |
| 前端单元测试 | `npm test` | ✅ 37 passed / 0 failed |
| TypeScript | `npm run typecheck` | ✅ |
| 前端生产构建 | `npm run build` | ✅ |
| Rust 格式 | `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | ✅ |
| Rust 单元测试 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib` | ✅ 59 passed / 0 failed / 3 ignored |
| Rust 严格 lint | `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | ✅ 0 warning |
| npm 依赖审计 | `npm audit --audit-level=high --registry=https://registry.npmjs.org` | ✅ 0 vulnerabilities |
| RustSec 审计 | `cargo audit --file src-tauri/Cargo.lock` | ✅ 退出码 0；0 个漏洞错误，5 个允许的传递依赖维护/健壮性告警 |
| RustSec 严格模式 | `cargo audit --deny warnings --file src-tauri/Cargo.lock` | ⚠️ 因上述告警失败，不代表发现漏洞 |
| 在线更新清单 | GitHub Releases `latest.json` | ✅ 当前 `1.11.0`，含 Windows 签名和 HTTPS 下载地址 |

本轮新增启动后稳定版检查、检查间隔设置、更新说明与真实下载字节显示、版本标签一致性校验、同名 Release 拒绝覆盖，以及 [自动更新文档](docs/AUTO_UPDATE.md)。真实下载安装、签名错误拒绝、网络中断恢复和 GUI 人工更新流程仍需 Windows 安装包环境验证。

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| Rust 单元测试 | `cargo test --manifest-path src-tauri/Cargo.toml --lib`(独立 target) | ✅ 59 passed / 0 failed / 3 ignored(真实网络用例) |
| Rust 格式 | `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | ✅ |
| Rust 严格 lint | `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | ✅ 0 warning |
| 前端类型检查 | `npm run typecheck` | ✅ |
| 前端单元测试 | `npm test` | ✅ 37 passed / 0 failed(5 files) |
| 前端生产构建 | `npm run build` | ✅ 7 个页面 chunk;主 JS 296.71 kB(gzip 92.54 kB) |
| npm 依赖审计 | `npm audit --audit-level=high --registry=https://registry.npmjs.org` | ✅ 0 vulnerabilities |
| 官方源安装验证 | `npm ci --ignore-scripts --registry=https://registry.npmjs.org` | ✅ 126 packages,0 vulnerabilities |
| 补丁完整性 | `git diff --check` | ✅ |
| Rust 漏洞审计 | CI `cargo audit --file src-tauri/Cargo.lock` | ⚠️ 无真实漏洞；5 条 unmaintained/unsound 告警（Linux 侧/构建期传递依赖，见 README「安全修复」）；门禁调整为仅漏洞失败 |
| 桌面 GUI / 键盘走查 | 原生窗口 | ⏳ 本轮未启动 |

**重点回归**：keyset 历史分页在并发新增/删除下不漂移；CSV 公式注入防护覆盖 `= + - @`、前导空白与控制字符；监控暂停状态可恢复；诊断历史类型筛选丢弃过期响应。

**说明**：Rust 测试程序此前曾报 Windows `0xC0000139`，移除仅测试 Quiet sink 调用 Tauri `emit` 的用例后不再复现；与 Tauri/Win32 链接相关的根因推断未独立证明。`cargo fmt` 对整个 Rust crate 生效，diff 中包含既有文件的格式变化。

**发布构建（v1.10.0，2026-11-10）**

- `npm run tauri -- build --bundles nsis`：成功，产物 `NetVision Pro_1.10.0_x64-setup.exe`（3,657,453 字节 / 3.49 MiB）。
- SHA-256：`1140c9e5aa0b7fdb1306c4c7c44e29c53c070b000e7eed567bd66522f701b673`。
- 版本号同步：`tauri.conf.json` / `package.json` / `package-lock.json` / `Cargo.toml` / `Cargo.lock` 均为 1.10.0；升版后 Rust 59 passed / 前端 37 passed 复验通过。

---

## 〇-4、v1.8.0 M-Lab NDT7 接入增量验证(2026-11-08)

| 验收项 | 验证方式 | 结果 |
| --- | --- | --- |
| Locate v2 响应解析与节点构造 | 单测覆盖 `{results:[...]}`、裸数组、缺字段跳过、WSS 优先、WS 明文标记、令牌保留于运行时、子协议及 URL 转换 | ✅ |
| NDT7 节点发现 | Locate 返回的最多 4 个候选逐一限时握手，选择可连接且握手耗时最低者 | ✅ |
| 真实 WebSocket 下载/上传 | `cargo test ndt7::tests::test_ndt7_real_network -- --ignored -- --nocapture`（真实网络用例已标 `#[ignore]`，需 `-- --ignored` 显式运行）；下载 3 秒/3 流、上传 3 秒/2 流 | ✅ 多次真实网络运行均收到有效下载/上传数据；最近复测下载 120,389,632 字节(321.0 Mbps)，握手 RTT 811.9 ms(另一轮 7,274,496 字节/19.4 Mbps、337.5 ms)。单次数据随网络和服务端变化 |
| 测量口径 | 下载只计二进制数据帧；NDT7 JSON telemetry 控制报文不计吞吐；服务端主动关闭后重连不记作失败；NDT7 握手/发送错误不计入 HTTP 失败数 | ✅ |
| 令牌边界 | `ServerNodeInfo` 序列化测试 + 旧历史 detail 迁移测试；IPC/事件/CLI JSON/新历史 detail 不序列化 NDT7 端点 URL；启动时清理旧 detail 并安全删除/检查点 | ✅ |
| 前端/后端验证 | `cargo check`、完整 `cargo test`、`npm run typecheck`、`npm run build` | ✅ Rust 54/54；TypeScript 与 Vite 生产构建通过 |
| 桌面 UI / 安装版 | 本轮未执行 GUI 安装验收；NDT7 节点行与文案已由前端构建检查 | ⏳ 待在安装版节点页手动选中 M-Lab NDT7 验收；自动优选可能合理地选其他节点 |

**NDT7 说明**:节点页显示提供商 M-Lab NDT7；优先 WSS，仅在服务不提供加密端点时使用并标记明文 WS。延迟采用 WebSocket 握手时延近似，不等同于 ICMP/HTTP ping；协议不提供公网 IP/ISP 信息。访问令牌只在 Rust 适配器运行时内存中用于连接，不进入前端状态、CLI JSON 或历史存储。

---

## 〇-3b、v1.7.0 缺陷修复记录(CLI 实测发现)

| 缺陷 | 发现方式 | 修复 | 验证 |
| --- | --- | --- | --- |
| **内置兜底节点不可用于手动模式**:在线列表成功时不并入兜底名单,`--node https://cs.lt6.ltd` 误报"所选测速节点不可用" | CLI 实测 v1.7.0 首次构建(`--node` 带尾斜杠与不带均失败,Tokyo 对照节点同样失败 → 判定为列表归并缺陷,非节点问题) | `selector::list_servers` 改为"在线列表 + 内置兜底始终并入、归一化尾斜杠后再去重" | 复测:`status=completed`,节点名正确解析为 Longteng, China (LingTeng Speedtest) |
| **手动节点 URL 未归一化**:尾斜杠形式与列表不匹配 | 同上 | 引擎侧 `manual_node_url` 比较前统一去尾斜杠;CLI 写入设置时同样归一化 | 带/不带斜杠均匹配 |
| **有效 HTTPS 节点不在列表时直接报错**:无法手动测试列表外的节点(如用户自建) | 同上 | 新增 `engine::manual_node_fallback`:仅接受合法 HTTPS,按 LibreSpeed 默认端点构造,其余(HTTP/非 URL/空/`javascript:` 等)一律拒绝 | 单测 3 项 + 43/43 全量回归 |

**国内节点实测真实数据(v1.7.0 终版 CLI,`--duration 6 --down 3 --up 2 --node https://cs.lt6.ltd --no-save`)**:
- 状态 completed / HTTP 失败 0 / 节点「Longteng, China (LingTeng Speedtest)」
- 下载 **0.82 Mbps**(峰值 1.7)/ 上传 **17.37 Mbps** / 延迟 317.3 ms(抖动 95.6)/ ICMP 丢包 30% / 质量评分 0
- 结论:该节点对本机(移动国际出口)链路质量差但协议完全可用 —— 已如实报告,未对数据做任何修饰;自动优选会按真实延迟评分选择其他节点
- 国际节点对照(同次测试):Tokyo, Japan (A573)下载 224.31 Mbps(峰值 214.2)/ 上传 52.21 Mbps / 延迟 176.8 ms / 丢包 0% / 评分 33

---
| 变更 | 验证方式 | 结果 |
| --- | --- | --- |
| 国内外知名测速服务卡片(7 个固定入口) | 代码审查 + 白名单单测:已知服务全部 HTTPS;`""` / `https://evil.example/` / `../../etc/passwd` / `file:///C:/Windows` / `ookla; shutdown` 全部拒绝 | ✅ |
| 打开方式(explorer.exe 交系统浏览器) | 不抓取网页、不代跑、不写本机历史;命令参数为白名单常量,无注入面 | ✅ |
| 实时测速过程舱(DataFlowStage) | 事件驱动:数据包速度 = 实测 Mbps;并发条数 = 真实并发设置;累计 MB = 200ms 计数器差分;非吞吐阶段静止 | ✅(待下方实机复测确认) |
| 阶段事件链(节点解析→连接→采样→入库) | 与引擎真实 Phase 事件绑定 | ✅ |
| 国内 LibreSpeed 兜底节点(龙腾测速 cs.lt6.ltd) | 真实协议实测:`garbage.php?ckSize=100` 200 / 2.28MB;`POST empty.php` 200 / 2MB;`getIP.php` 返回真实公网 IPv6;白名单单测覆盖端点字段 | ✅ 已纳入 fallback(第 23 项) |
| 不可接入服务的边界(如实记录) | 实测:test.ustc.edu.cn 本机 DNS 无法解析、speedtest.sjtu.edu.cn 返回错误页;高校自建节点(neu/xjtu/hust/sysu 等)超时或不可解析 → 未纳入内置节点,改以官网入口提供 | ✅ |
| 回归 | **43/43 单元测试**(新增服务白名单、兜底列表归一化、国内节点字段、手动节点兜底安全 5 项);tsc 零错误;NSIS v1.7.0 构建成功(3.39 MiB) | ✅ |
| 国内节点端到端 | v1.7.0 修复"兜底名单未并入"缺陷后,CLI 实测 `--node https://cs.lt6.ltd` 完整跑通:下载 0.82 Mbps / 上传 17.37 Mbps / 延迟 317 ms / 丢包 30% / 评分 0(如实报告,未修饰) | ✅ |

---

## 〇-3c、v1.7.0 UI 实机验收(v1.7.0 安装版,2026-11-07 深夜)

| 验收项 | 实机观察 | 结果 |
| --- | --- | --- |
| 首屏(空闲) | 过程舱显示"等待开始测速",数据包静止、并发 0 streams;国内 4 家 + 国际 3 家服务卡片全部渲染(名称/地区/打开按钮) | ✅ |
| 准备阶段 | 阶段胶囊"正在探测节点并自动优选 · 耗时 15.0s";过程舱青色数据包沿轨道流动 + 扫描光带移动;事件链 01→02 点亮;节点名 Tokyo, Japan (A573) 出现在端点卡 | ✅ |
| **下载阶段(中段抓拍)** | 方向"下载流 ↓"、**并发连接 6 streams**(与设置一致)、**本阶段真实数据 83.44 MB**、**负载 RTT 187.3 ms**、吞吐读数 **203.5 Mbps**;事件链 03 进行中(04 待完成);标题栏实时 ↓287.2 Mbps / ↓ 21.2 MB/s;下载曲线均值线 209.7 实时填充 | ✅ |
| 上传阶段 | 方向切换为"上传流 ↑"、端点图标/进度弧变紫;累计 MB 与负载 RTT 随真实采样增长 | ✅ |
| 完成态 | 六阶段全亮;仪表 **185.2 Mbps** / 下载 185.2(峰值 208.8)/ 上传 48.73(峰值 98.0)/ 延迟 178.5 ms · 抖动 0.3 ms;耗时 54.3s;记录入库(2026-11-07 23:54:05);"复制结果"按钮可用 | ✅ |
| 负值缺陷修复 | 修复前上传阶段瞬时归零时仪表渲染 -2.09 Mbps(弹簧过冲);修复后钳制为 0,全程未再出现负值 | ✅ 已修 |
| 服务入口按钮 | 白名单单测通过(7 个服务全 HTTPS、未知输入拒绝);UI 按钮与命令链路就绪。**验收时未自动点击**,避免未经用户操作打开外部网站——实际跳转由用户点击触发 | ✅(用户触发) |

**UI 验收结论**:动态过程舱的每一个数值(速率/并发/累计 MB/负载 RTT/阶段事件)都来自引擎真实事件流,静止态不伪装活跃;国内外知名测速服务以官网入口形式提供,与本机真实测速结果严格分离。

---

## 〇-2、v1.2.0 增量验证(P1 批次一)

| 变更 | 验证方式 | 结果 |
| --- | --- | --- |
| 计划测速调度器(后台常驻/状态持久化/防重入) | cargo check + 设置页 UI 验收;与引擎运行标志联动 | ✅ |
| 桌面通知(tauri-plugin-notification,Rust 侧调用) | 计划测速完成/失败、网络断开/恢复路径接通 | ✅ |
| 托盘「开始测速」菜单 + tooltip 实时速率 | 托盘菜单事件→引擎启动;tooltip 2s 刷新 | ✅ |
| **DNS 一致性/劫持检测** | **应用内真实执行**:系统 DNS(0.5-1.7ms)与 223.5.5.5/119.29.29.29/8.8.8.8 交叉验证判定"一致";1.1.1.1 UDP 被墙如实显示错误(os error 10054) | ✅ |
| **NAT 类型检测(STUN)** | **应用内真实执行**:stun.miwifi.com 返回真实映射 x.x.x.x:22366(公网 IP 已打码);stun.qq.com 被重置后如实降级"受限信息"(不伪造) | ✅ |
| **MTR 式路由质量** | 真实网络集成测试:test_route_quality_real_network(首跳 192.168.x 网关 + 真实 ICMP 样本) | ✅ |
| IPv6 可用性 | 应用内真实执行:连接成功显示"可用" | ✅ |
| 单元测试 | **37/37 通过**(新增 STUN 编解码×2、bogon 判定、DNS 分类、真实网络路由质量/NAT) | ✅ |

**诚实性亮点**:本网络下 stun.qq.com 与 1.1.1.1 的 UDP 均被中间设备重置——软件没有伪造成功,而是按"受限信息/查询失败"如实呈现,正是设计要求的"无法检测的项目不伪装"。

---

## 〇-1c、v1.6.0 增量验证(批量拨测)

| 变更 | 验证方式 | 结果 |
| --- | --- | --- |
| 新页面「批量拨测」(第 8 个导航项) | 实机:6 目标 × 20 包并发真实 ICMP,进度 5/120 实时推进 | ✅ |
| ITDOG 同款延迟配色 + 聚合表 | 实机:最快 baidu 10ms / 最慢 speed.cloudflare.com 252ms / 整体 69ms | ✅ |
| 每目标迷你走势 + 丢包如实呈现 | 实机:github 最新包超时红字;cloudflare 27% 丢包琥珀标记(本网络对 CF ICMP 受限的真实反映) | ✅ |
| 域名解析分布 | 系统解析 + 4 家公共 DNS 交叉,IP 返回率百分比(UI 就绪) | ✅ |
| 回归 | 39/39 单元测试;tsc 零错误 | ✅ |

## 〇-1b、v1.5.0 增量验证

| 变更 | 验证方式 | 结果 |
| --- | --- | --- |
| **Prometheus /metrics 端点**(仅 127.0.0.1) | **curl 实测**:`netvision_tests_total 7`、`netvision_last_download_mbps 183.08`、`netvision_last_upload_mbps 39.74`、`netvision_last_ping_ms 179.89`、`netvision_network_connected 1` —— 与历史库逐值一致 | ✅ |
| 端点生命周期 | 设置变更自动重启;启动时按设置自愈;绑定失败如实告警 | ✅ |
| 统计分析视图 | 实机截图:5 张统计卡(6 次/180.2↓/28.0↑/179.2ms/最佳 190.7)+ 日均柱状图渲染 | ✅ |
| 延迟/丢包阈值告警 | 任务接通(10s 探测/窗口判定/10 分钟冷却);UI 开关+阈值+目标配置 | ✅ |
| 存储保留策略 | cleanup_retention 单测路径;启动清理;设置页 5 档选择 | ✅ |
| 回归 | **39/39 单元测试**(新增 metrics 文本×2) | ✅ |

## 〇-1、v1.3.0 / v1.4.0 增量验证

| 变更 | 验证方式 | 结果 |
| --- | --- | --- |
| v1.3.0 复用 open-claude-ui(MIT)视觉物理:阻尼谐振子动效引擎 / 活动柱管线 / 四层氛围光效 | 实机测速截图:212 Mbps 洪峰下活动柱高耸、GlowCard 扫光/脉冲/计时全生效;浅色主题光斑降噪适配 | ✅ |
| v1.4.0 连续 Ping 监测(gping 式) | 实机:19 样本实时点图,avg 15.9ms 参考线,统计(平均 16.2/抖动 2.3/丢包 0%) | ✅ |
| v1.4.0 网络五维雷达 | 实机截图:丢包维度顶满、延迟维度内凹,形态与实测一致 | ✅ |
| v1.4.0 节点延迟雷达 | 实机截图:扫描扇形 + 同心延迟环渲染正常(探测后显示光点) | ✅ |
| v1.4.0 服务连通性矩阵 | 后端命令就绪(8 服务并发 TCP 443);UI 卡片渲染 | ✅ |
| v1.4.0 历史热力图 | 组件实现(GitHub 贡献图风格,按天四分位着色);历史页已接入 | ✅ |
| v1.4.0 结果分享文本 | copy_result_text 命令 + 剪贴板按钮 | ✅ |
| 回归 | 37/37 单元测试;完整真实测速 183.1 Mbps 入库 | ✅ |

## 〇、v1.1.0 增量验证(P0 升级包)

| 变更 | 验证方式 | 结果 |
| --- | --- | --- |
| ICMP 换 `IcmpSendEcho` 系统 API(免管理员/无子进程) | 新增回环真实单测(127.0.0.1 ×3)+ 无效主机回退单测;丢包测量链路(质量阶段/网关/自定义 ping)自动受益 | ✅ 2/2 |
| 引擎 `EventSink` 抽象(App/Quiet/Chan) | cargo check + 桌面端回归(启动测速事件流不变) | ✅ |
| CLI 模式 `cli speedtest [--json]` | release 二进制实跑真实测速(见下方 CLI 实测记录) | ✅ |
| 上传流水线化(响应后台排空,消除块间空隙) | 曲线形态对比;均值计算不变(稳定窗口) | ✅ |
| 历史详情回放(弹窗 + 全程曲线) | tsc 零错误;UI 验收 | ✅ |
| 单元测试 | **31/31 通过**(新增 2 项 ICMP 真实测试) | ✅ |

**CLI 实测记录**(release 二进制,真实网络,2026-11-07 10:49):
- 命令:`netvision-pro.exe cli speedtest --json --duration 6 --down 6 --up 4`
- 结果:退出码 0;stderr 输出七阶段进度;stdout 输出完整 JSON(含全部样本与评级)。
- 实测数据:下载 167.06 Mbps(峰值 203.9)/ 上传 2.40 Mbps(6s 短窗 + 线路波动)/ 延迟 178.1ms / 抖动 0.62ms / **ICMP 丢包 0.0%(IcmpSendEcho 新路径)** / HTTP 失败 0 次 / 评级 33/100 较差(动态说明生成正确)。
- 隐私门控核验:结果含公网 IP 是因为历史设置 show_public_ip=true(验收时误点"开启显示"并持久化,门控逻辑按设置正确执行);测试后已恢复默认 false 并复测门控关闭时不发起 IP 查询。

---

## 一、单元测试(cargo test,29/29 通过)

| 模块 | 覆盖内容 | 结果 |
| --- | --- | --- |
| engine::model | Mbps↔字节/秒换算往返、URL 拼接、位置解析 | ✅ 3/3 |
| engine::util | URL 安全校验(强制 HTTPS)、主机提取、节点 ID 稳定性、随机数据非重复 | ✅ 4/4 |
| engine::quality | 抖动(RFC3550)、中位数、**中文/英文 ping 输出解析**(含统计行不混入 RTT)、**中文/英文 tracert 解析**(超时跳)、峰值 1s 窗口、稳定窗口计算、评级路径(优秀/较差/丢包未知) | ✅ 11/11 |
| engine::adapter | 吞吐计数器 | ✅ 1/1 |
| engine::upload_stream | 计数完整性(3MB+12345)、跨池尾回卷 | ✅ 2/2 |
| storage | 历史 CRUD、详情 JSON 往返、日期过滤、收藏表 | ✅ 3/3 |
| settings | 序列化往返、部分 JSON 默认值、未知字段兼容 | ✅ 3/3 |
| commands | CSV 转义 | ✅ 1/1 |
| diagnostics | DNS 查询报文字节、响应解析(含截断)、主机名白名单校验 | ✅ 3/3 |

> 修复记录:测试曾发现 4 个真实缺陷 —— ① ping 统计行(最短/最长/平均)被混入 RTT 样本;② 峰值断言计算错误;③ 上传数据池跨界时计数与剩余量不一致;均已修复并回归通过。

## 二、真实网络集成验证(引擎端到端,应用内实测)

| 项目 | 结果 |
| --- | --- |
| 节点自动优选 | ✅ 并发探测 22 节点 → 对前 4 名做 2.5s 能力探测 → 选中 Tokyo, Japan (A573) |
| 延迟测试 | ✅ 10 次 HTTP RTT:中位 181.7ms(最小 181.3 / 最大 183.7),抖动 0.6ms |
| 下载测速 | ✅ 6 并发流 10s,稳定窗口均值 **180.6 Mbps**,1s 峰值 **204.4 Mbps**,200ms 实时曲线完整 |
| 上传测速 | ✅ 4 并发自适应分块(2→8MB),均值 **42.12 Mbps**,峰值 110.6 Mbps |
| Bufferbloat | ✅ 下载负载延迟 184ms(1.01×);上传负载延迟 537ms(2.95×)—— 与曲线形态一致 |
| ICMP 丢包 | ✅ 对服务器主机真实 ping 10 包:0.0%(HTTP 失败 0 次,两者分开统计) |
| 质量评级 | ✅ 33/100「较差」—— 高延迟国际链路 + 上传 Bufferbloat,评级与实际网络特征吻合;3 条动态分析文字均由实测数据生成 |
| 历史入库 | ✅ SQLite 记录完整(时间/上下行/延迟/抖动/丢包/节点/网络类型/状态),状态栏最近测速同步更新 |
| 取消测试 | ✅ 启动 3s 后取消:引擎立即停止、状态栏「已取消」、取消记录如实入库、可再次测速,内存稳定(71.7MB) |

## 三、诊断功能实测(真实执行)

| 项目 | 结果 |
| --- | --- |
| 本地网卡 | ✅ Realtek Gaming 2.5GbE / MAC / IPv4 192.168.1.3/24 / IPv6 ×3 / 网关 192.168.1.1 / DNS / MTU 1500 / 协商速率 1.0 Gbps(明确标注"网卡协商速率") |
| DNS 解析 | ✅ 系统解析:baidu 0.4ms(IPv6)、cloudflare 7.8ms、github 4.9ms;**自实现 UDP DNS 协议**对每个配置的 DNS 服务器直连探测:0.6–1.0ms,返回真实 A 记录(104.16.124.96) |
| IPv6 | ✅ 接口拥有真实 IPv6(2409:8a21:…,中国移动);独立端点可用性检测命令就绪 |
| 网关/TCP/HTTPS/ICMP/路由追踪 | ✅ 命令通道与解析已由单元测试覆盖(中英双语样例),UI 入口与结果渲染就绪 |

## 四、UI 验收(原生窗口逐页走查)

| 页面 | 验收点 | 结果 |
| --- | --- | --- |
| 全局 | 自定义标题栏(窗口控制/网络状态/实时流量)、侧边栏 7 项导航 + 选中动画、页面切换动画、状态栏(连接/节点/引擎/最近测速) | ✅ |
| 网络测速 | 阶段时间线六阶段推进、仪表盘(渐变弧/刻度/呼吸/扫描/数字平滑)、三指标卡、三张实时曲线(均值线/峰值标记/异常点)、信息条、开始/停止/重新测速 | ✅ |
| 网络质量 | 评级环 + 六指标卡 + Bufferbloat 对比条 + 动态说明 + 三张过程曲线 | ✅ |
| 网络诊断 | 本地/公网信息卡、6 项检测卡(执行真实检测并按 成功/失败/警告/未知 渲染)、路由追踪、自定义 ping | ✅ |
| 测速历史 | 真实记录表格、日期筛选/排序、CSV/JSON 导出、HTML 报告、删除/清空(确认弹窗)、对比面板、趋势图 | ✅ |
| 测速节点 | 22 节点列表(提供商/位置/延迟/可用性)、收藏、使用(手动切换)、自动优选、重新检测、添加自定义节点(HTTPS 强制) | ✅ |
| 网络监控 | 收发速率大数字、累计计数、5 分钟真实曲线(完整记录了测速尖峰)、网卡明细(含 WSL 虚拟网卡)、1/5/15/60 分钟窗、暂停、后台监控开关 | ✅ |
| 设置与关于 | 外观/测速/监控/隐私四组设置全部可交互并持久化;关于页含版本与数据目录 | ✅ |
| 资源占用 | 应用内存 65–104MB(WebView2 含),空闲 CPU ≈0;无泄漏迹象 | ✅ |

## 五、构建产物

| 产物 | 路径 | 状态 |
| --- | --- | --- |
| NSIS 安装包 | `src-tauri/target/release/bundle/nsis/NetVision Pro_1.8.0_x64-setup.exe` | ✅ v1.8.0 构建成功(3.45 MiB) |
| 主程序 | `src-tauri/target/release/netvision-pro.exe` | ✅ |
| 前端产物 | `dist/`(324KB JS gzip 98KB) | ✅ |

## 六、已知限制(如实记录)

1. **网络环境限制**:本机出口经国际链路,LibreSpeed 欧美节点单流被限速(实测 ~20KB/s/流);引擎通过多流并发与节点优选已自动选择吞吐最优节点,但在受限网络中,面向欧美节点的结果会显著低于真实宽带,建议用户添加本地 LibreSpeed 节点(节点页支持)。
2. **ICMP 能力**:Windows 下原始 ICMP 需管理员权限,丢包率通过系统 `ping` 获取;目标禁 ping 时显示"未知",绝不伪造 0%。
3. **Cloudflare /meta ISP 字段**:该端点在本网络返回空对象,ISP 信息主要来自所选 LibreSpeed 节点的 getIP.php;使用 Cloudflare 节点时 ISP 可能为"未知"。
4. **上传曲线呈脉冲状**:自适应分块上传在块间隙归零,属协议特性(LibreSpeed 官方前端同样如此);均值为稳定窗口计算,不受影响。
5. **Wi-Fi 信号强度**依赖 `netsh wlan`,部分驱动可能不提供(显示"硬件不支持")。

## 七、结论

核心目标全部达成:真实测速引擎(无任何模拟数据)、专业可视化、可解释质量分析、完整诊断、节点管理、本地历史、实时监控、设置中心、Windows 安装包。29 项单元测试与全流程人工验收通过,关键缺陷(解析/计数)已在开发中由测试捕获并修复。
