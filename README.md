# NetVision Pro

**专业级 Windows 桌面网络测速与诊断软件**

真实测速引擎 · 可解释网络质量评级 · 完整网络诊断 · 实时流量监控 · 本地历史数据库

---

## 功能总览

| 模块 | 能力 |
| --- | --- |
| 网络测速 | 多流并发真实测速(延迟 → 下载 → 上传 → 质量分析),实时速度曲线,可安全取消 |
| 测速过程可视化 | 实时数据流舱:数据包按实测速率流动 / 并发连接条 / 累计字节 / 阶段事件链,静止时不伪装活跃 |
| 知名测速服务 | 国内外官方测速站入口(SpeedTest.cn、三大运营商、Ookla、FAST.com、Cloudflare)—— 固定白名单 HTTPS 官网跳转,不在本机抓取或代跑 |
| 网络质量 | Bufferbloat(负载延迟)、抖动、ICMP 实测丢包、可解释的 优秀/良好/一般/较差 评级 |
| 网络诊断 | 网卡/网关/DNS(自实现 UDP 协议探测)/TCP/HTTPS/IPv6/ICMP/路由追踪 |
| 测速节点 | LibreSpeed 官方在线列表 + 内置兜底 + Cloudflare 边缘节点 + M-Lab NDT7(Locate v2 动态发现) + 自定义节点;自动优选(延迟 65% + 下载能力 35%) |
| 测速历史 | SQLite 本地存储;日期筛选/排序/对比/趋势图/CSV/JSON 导出/HTML 报告 |
| 批量拨测 | 多目标并发真实 ICMP 拨测,ITDOG 同款延迟配色/聚合表/走势,域名解析分布 |
| 网络监控 | 网卡层真实速率(1s 采样,1 小时环形缓冲),1/5/15/60 分钟时间窗,可暂停/关闭 |
| 设置中心 | 主题/动画/测速参数/并发数/自启动/隐私开关 —— 全部真实生效 |

## 技术栈

- **后端**:Rust + Tauri 2(WebView2)
- **测速协议**:LibreSpeed(HTTP 多流)+ Cloudflare Speed(边缘网络)+ M-Lab NDT7(WebSocket);国内外知名测速服务以官方官网入口方式提供(见"已知限制")
- **前端**:React 19 + TypeScript + Zustand + 自研 Canvas 图表/仪表盘(零图表库依赖)
- **存储**:rusqlite(SQLite WAL)+ JSON 设置(原子写入)
- **系统**:sysinfo(网卡流量)、netdev(接口/网关/DNS)、tauri-plugin(single-instance / autostart / dialog / log)

## 架构

```
src-tauri/src/
├── engine/            测速引擎(与 UI 完全分离)
│   ├── mod.rs           编排:防重复启动/取消/阶段推进/采样/看门狗
│   ├── adapter.rs       提供商抽象 + 统一 HTTP 延迟测量
│   ├── librespeed.rs    LibreSpeed 协议(多流下载/自适应分块上传)
│   ├── cloudflare.rs    Cloudflare Speed 协议(边缘网络)
│   ├── ndt7.rs          M-Lab Locate v2 + NDT7 WebSocket 适配器
│   ├── selector.rs      节点列表(在线+兜底)/并发探测/自动优选
│   ├── quality.rs       抖动/中位数/ICMP 解析/可解释评级
│   ├── upload_stream.rs 随机数据流(64KB 粒度真实计数)
│   └── model.rs         统一数据模型与单位换算
├── diagnostics.rs     网络诊断(netdev + 原始 DNS UDP + 系统 ping/tracert 语言无关解析)
├── monitor.rs         实时监控(1h 环形缓冲,状态变化事件)
├── storage.rs         SQLite 历史库 + 节点收藏
├── settings.rs        设置(serde default,原子持久化)
├── report.rs          HTML 测速报告
└── commands.rs        Tauri 命令层(前端唯一入口)

src/                   前端(React)
├── store/             Zustand 状态(设置/测速事件机/监控/节点)
├── components/        Gauge(环形仪表)/ LineChart(Canvas 曲线)/ 外壳组件
└── pages/             7 个功能页面
```

## 测速方法学

1. **准备**:接口检查 → 节点列表(在线优先,失败用内置快照)→ 并发探测(2×HTTP RTT)→ 候选前 4 名做 2.5s 下载能力探测 → 综合评分选定。
2. **延迟**:10 次(精度模式 16 次)HTTP 往返,输出中位数/最小/最大;单次失败跳过。
3. **下载**:N 条并发流持续拉取 `garbage.php?ckSize=100`(Cloudflare 为 25MB 块),200ms 粒度流式计数;**剔除 1.2s TCP 慢启动预热期**计算稳定均值;峰值取 1s 滑动窗口最大均值;计数器停滞 4s + 存在失败 → 判定网络中断并提前结束。
4. **上传**:随机数据池按 2MB 起步自适应分块(413 自动减半),64KB 粒度真实计数;上传整体失败不终止测试,如实标注。
5. **质量**:下载/上传阶段内每 500ms 并行测负载延迟(Bufferbloat);对服务器主机真实 ICMP 10 包测丢包;**HTTP 请求失败与网络丢包严格分开统计**。
6. **评级**:综合空闲延迟、抖动、负载延迟比、丢包、HTTP 失败动态打分(0-100)并生成针对性文字说明。

## 单位约定

内部一律字节/秒;`Mbps = bytes × 8 / 1e6`(见 `engine/model.rs`,有单元测试)。网卡协商速率仅作展示,绝不与宽带速率混淆。

## 构建与运行

依赖:Node 18+、Rust 1.80+(MSVC)、WebView2(Windows 10/11 自带)。

```bash
npm install
npm run icons        # 生成应用图标(仅首次)
npm run tauri dev    # 开发运行
npm run tauri build  # 构建 NSIS 安装包(src-tauri/target/release/bundle/nsis/)
npm run test         # 前端单元测试(vitest)
cargo test           # 后端单元测试(src-tauri 下执行);真实网络用例已标 #[ignore],
                     # 需显式运行:cargo test -- --ignored
```

## CLI 模式(v1.1.0+)

与桌面端共用同一引擎与历史库,适合脚本化:

```bash
netvision-pro.exe cli speedtest --json --duration 10 --down 6 --up 4 [--node <url>] [--precision] [--no-save]
```

- 进度 → stderr;结果 → stdout(`--json` 为完整 JSON,含实时样本与质量评级)
- 退出码:`0` 完成 / `2` 已取消 / `3` 失败 / `1` 参数错误
- 结果按设置自动入库(历史页可见;`--no-save` 可跳过)

## 可视化致谢

动效物理与部分可视化模式复用自 [Caho1/open-claude-ui](https://github.com/Caho1/open-claude-ui)(MIT License)对 Claude 桌面端的逆向成果:

- **阻尼谐振子动效引擎**(`src/lib/motion.ts`):二阶阻尼弹簧数值积分,驱动仪表大数字与进度弧(微弹超调/零过冲两档预设)
- **实时活动柱**(`src/components/ActivityBars.tsx`):归一化 → 噪声门限 → Gamma 0.7 压缩管线,数据源为真实网卡速率历史(标题栏/监控页)
- **氛围光效**(`src/components/GlowCard.tsx`):测速中卡片四层光效——1.8s 呼吸光晕、三相位脉冲点(0.2857 黄金常数)、非对称扫光文字、1.1s 线性能量扫光条

## v1.8.0 变更摘要

- **M-Lab NDT7 接入**:通过 Locate API v2 动态发现 NDT7 服务，对最多 4 个候选执行 WebSocket 握手并选择响应最快者；使用 `net.measurementlab.ndt.v7` 子协议执行真实二进制下载/上传，复用统一的阶段、采样和结果链路。
- **安全与测量口径**:优先 WSS；只有没有加密端点时才使用 WS 并在节点页标记未加密。Locate 访问令牌只留在适配器运行时内存，节点/事件/CLI JSON/历史序列化均不输出端点 URL；启动时清理旧历史 detail 中的 NDT7 URL，并清理 SQLite 页面。Telemetry JSON 控制帧不计吞吐，正常服务端关闭不算传输失败。
- **延迟与隐私说明**:NDT7 延迟使用 WebSocket 握手耗时近似，不是独立 ICMP/HTTP ping；NDT7 不提供公网 IP/ISP 接口，因此对应字段留空。
- **节点页**新增 M-Lab NDT7 提供商展示；手动选择节点可进行 NDT7 测速。自动优选仍依据实测延迟与下载能力，结果可能选择其他协议节点。

## v1.7.0 变更摘要

- **国内外知名测速服务入口**:测速页新增「国内外知名测速服务」卡片(国内:SpeedTest.cn、广东电信、联通、北京移动;国际:Ookla Speedtest、FAST.com、Cloudflare Speed)。
  Rust 侧 `open_speedtest_service` 只接受 7 个固定服务 ID → 恒定 HTTPS URL 白名单,经 `explorer.exe` 交系统浏览器打开;**不接受任意 URL、不抓取网页、不代替网站执行测试**,第三方站点结果不会写入本机历史。
- **实时测速过程舱**(`DataFlowStage`):数据包沿轨道往返流动,速度由实测 Mbps 驱动(越快越密);并发连接条数 = 设置中的真实并发数;显示本阶段真实累计 MB / 负载 RTT / 最近 RTT;四个阶段事件(节点解析 → HTTPS 连接 → 样本采集 → 评分入库)按真实阶段推进点亮。
  非吞吐阶段(延迟 / 质量)或未运行时数据包静止,**绝不伪装活跃**。
- **兜底节点新增实测可用的中国大陆 LibreSpeed 节点**(`cs.lt6.ltd`,龙腾测速):`garbage.php` 下载 / `empty.php` 上传 / `getIP.php` 均实测返回 200;已并入兜底名单(在线列表成功时也始终并入、归一化尾斜杠去重)。
  该节点对本机链路实测:下载 0.82 Mbps / 上传 17.37 Mbps / 延迟 317 ms / 丢包 30%(质量差但协议可用,自动优选中会按真实评分选择其他节点)。
- 单元测试 **43/43**(新增服务白名单、兜底名单归一化/国内节点字段、手动节点兜底安全校验)。

> v1.7.0 实测修复:在线列表成功时未并入内置兜底名单,导致手动 `--node` 与国内节点不可用;已改为"在线 + 兜底始终并入、归一化尾斜杠后去重",并为列表外的合法 HTTPS 节点增加了按 LibreSpeed 默认端点构造的兜底(仅 HTTPS,其余一律拒绝,由真实请求验证)。

## v1.1.0 变更摘要

- **ICMP 换系统 API**(`IcmpSendEcho`):免管理员、无子进程、毫秒级精度;IPv6/异常场景自动回退系统 ping
- **CLI 模式**:引擎事件层抽象(`EventSink`),桌面与 CLI 复用同一测速实现
- **上传流水线化**:响应后台排空,消除分块上传的块间空隙(均值口径不变)
- **历史详情回放**:历史页每条记录可查看全程样本曲线与完整指标


## 隐私与安全

- 前端不直接访问网络,所有 I/O 在 Rust 侧完成;CSP 限制 IPC 之外的一切远程资源。
- 公网 IP/ISP 仅在用户明确开启(`show_public_ip`)后查询,且只使用与测速同源的 Cloudflare 端点与所选 LibreSpeed 节点。
- 外部测速网站入口为**固定白名单**(7 个服务 ID → 恒定 HTTPS URL),非通用 URL 打开器;由系统浏览器打开,不在本机抓取页面或上传任何数据。
- 子进程仅 `ping`/`tracert`/`explorer.exe`(URL 协议处理,参数为白名单常量),无命令注入面;不需要管理员权限。
- 日志不记录 IP/主机名;设置与历史均在本机,卸载不自动删除数据。

## 已知限制

- 无 ICMP 原始套件能力(Windows 需管理员),丢包率通过系统 `ping` 获取;目标禁 ping 时显示"未知"而非伪造 0%。
- 节点池以国际节点为主,国际链路被限速的网络环境测速结果会明显低于实际带宽,建议添加本地自定义节点。v1.7.0 兜底列表已补入一个实测可用的国内 LibreSpeed 节点(龙腾测速),但其可达性与延迟随网络环境变化,自动优选中若延迟劣后仍会选其他节点。
- **国内外"著名测速服务"的接入边界(如实说明)**:Speedtest.cn、三大运营商、Netflix FAST、Ookla 等均**未公开面向第三方的稳定测速 API**(FAST 的内部端点需要从其页面 JS 取 token,Ookla 的 TCP 协议受其服务条款约束、非官方逆向存在合规风险)。因此这些服务仍以"官方入口 + 系统浏览器"方式提供，不抓取网页、不模拟其结果；应用内协议测速包括 LibreSpeed、Cloudflare Speed 与公开的 M-Lab NDT7。NDT7 延迟是 WebSocket 握手时延而非相同条件下的 ICMP/HTTP ping，且不返回公网 IP/ISP。实测记录:中科大测速站本机无法解析、上海交大入口返回错误页,高校自建节点在国内公网普遍限校访,均未纳入内置节点。
- Wi-Fi 信号强度依赖 `netsh wlan`,部分网卡/驱动可能不提供。
