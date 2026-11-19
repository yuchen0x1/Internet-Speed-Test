# 平台、CLI 与更新策略

评估日期：2026-11-18。Windows 桌面是完整产品；CLI 是独立控制台工具，复用同一 Rust 测速引擎、节点协议、SQLite 格式和历史筛选。CLI 不声称提供桌面 UI、托盘或后台通知。

## 平台决策

每行综合评估技术与核心兼容性、体验、权限/硬件、框架、开发/维护成本、用户价值、分发和更新。A/B 表示纳入本轮工程范围，**不代表已经通过设备验收**。

| 平台 | 决策 | 核心能力、体验与权限 | 成本、价值、分发及更新 |
| --- | --- | --- | --- |
| Windows 10 x64 | A 桌面+CLI | 完整测速/诊断/监控；WebView2；ICMP 系统 API；当前用户安装 | 延续现有产品；NSIS 与签名更新；Win10 真机安装待验收 |
| Windows 11 x64 | A 桌面+CLI | 同上，保留键盘、窗口、托盘和通知 | 本机运行及自动构建；Authenticode 凭据缺失 |
| Linux（通用） | B CLI；C 桌面 | 无显示服务器依赖；普通用户运行；ping 工具及权限影响 ICMP；不提供 Windows Wi-Fi 明细 | Rust 核心共享，成本低、服务器价值高；tar.gz 签名分发，人工验证替换 |
| Ubuntu 22.04+ x64 | B CLI | runner 原生编译及进程/SQLite 冒烟；glibc/OpenSSL 系统运行库 | Ubuntu 22.04 为构建基线；cron/systemd 外部调度 |
| Debian 12+ x64 | B CLI | 与 Ubuntu 共享 glibc 构建；需 libssl3、CA 与 iputils-ping | 不另养桌面 DEB；实际发行版运行待验证 |
| Fedora x64 | B CLI | 原生工具权限与 glibc/OpenSSL ABI 需发行版验收 | 共享归档；RPM 仓库签名与发行维护成本高，本轮不发布 RPM |
| Arch Linux x64 | B CLI | 滚动运行库变化需持续检查 | 可从源码构建；预编译包真机待验证，不声称 AUR/Flatpak 支持 |
| macOS Intel | B CLI；C 桌面 | 原生 runner 产物；BSD ping 毫秒超时参数；无 WebView 依赖 | CLI 成本低；桌面原生菜单/窗口、Developer ID、公证与设备 QA 尚缺 |
| macOS Apple Silicon | B CLI；C 桌面 | 原生 arm64 runner；与 Intel 分开命名 | 不混淆架构；归档 updater 签名不等于 Apple 签名或公证 |
| Android 手机 | C | 可重做触控 UI，但后台、网卡、系统诊断与桌面不同；需 JNI/权限适配 | 新产品级维护成本高，当前无 Android 设备验收和签名生命周期，暂不开发 |
| Android 平板 | C | 需要双栏触控布局及横竖屏、后台适配 | 在 Android 核心产品形成前不单独开发 |
| 折叠屏 | C | 折叠状态、多窗口和安全区域需设备测试 | 目前无设备与 Android 基础，暂缓 |
| iOS | C | 沙盒、ICMP和后台调度限制，不能完整照搬桌面 | 需独立原生体验、Apple 账户与 App Store 更新；暂缓 |
| iPadOS | C | 同 iOS，另需多窗口和文件授权设计 | 不用网页套壳宣称支持，暂缓 |
| 桌面 Web | D（完整产品） | 无任意 ICMP、系统网卡计数器/路由/本地 SQLite 权限；CORS 也限制测速节点 | 服务端测速只能测服务器链路，无法代表用户本机；不发布失真的完整 Web 版 |
| 移动 Web | D（完整产品） | 同 Web，另有后台与浏览器流量限制 | 不做与桌面功能等价的宣传 |
| 响应式 Web | C（有限测速产品） | 只能重新设计 HTTP 测速子集，不能承诺系统诊断 | 存在明确功能缩水，当前收益不足以单独运营 |
| PWA | D（完整产品） | 离线缓存不能赋予系统诊断能力；后台任务仍受浏览器限制 | 不添加无价值 Service Worker 壳层 |
| Chrome 扩展 | C | 浏览器侧可测 HTTP，系统测量仍需 native messaging 与安装本机服务 | 同时维护扩展商店和本机服务收益低 |
| Edge 扩展 | C | 同 Chrome，独立商店发布与权限审核 | 继续使用桌面工具，无额外扩展 |
| Firefox 扩展 | C | WebExtensions 限制及额外适配维护 | 本轮不开发 |
| Docker | C | 测得容器网络，不等于宿主网卡；ICMP 权限/网络 namespace 改变口径 | CLI 可原生运行，无必要引入镜像供应链与特权容器 |
| NAS | B Linux CLI / C 专用包 | x64 Linux NAS 可源码构建；ARM、精简 libc 与厂商系统未验证 | 不伪称群晖/QNAP 商店支持；保留 CLI 退出码与外部调度 |
| 私有化部署 | B CLI | 本地数据，无新增云服务/数据库；支持指定挂载数据目录 | 原生安装、离线历史导出与手动签名升级 |
| 云服务器 | B CLI | 测量服务器所在链路；无 GUI；不等同远端用户网络 | 同 Linux CLI，不暴露未经认证的远程控制 API |
| CLI（四种原生架构产物） | A Windows / B Unix | stdout JSON/CSV，stderr 进度，稳定退出码；核心协议与桌面一致 | CI 原生构建与签名归档；更新见下文 |

## 独立 CLI

从 Release 下载 `NetVision.Pro_<版本>_cli-<windows-x64|linux-x64|macos-x64|macos-arm64>.<zip|tar.gz>`。解压后运行：

```sh
./netvision-cli --version
./netvision-cli speedtest --json --duration 5 --no-save
./netvision-cli history list --json
./netvision-cli history export --format csv --from 2026-11-01 --to 2026-11-18
```

Windows 使用 `netvision-cli.exe`。原桌面程序的 `netvision-pro.exe cli ...` 仍兼容。

进度写 stderr；结果写 stdout；退出码：0 完成、1 参数错误、2 取消、3 执行失败。测速会产生真实上下行流量；服务器/NAS 结果代表运行位置。

数据目录按 `NETVISION_DATA_DIR`（显式覆盖）→ Windows `%APPDATA%/com.netvisionpro.app` / macOS `~/Library/Application Support/com.netvisionpro.app` / Linux `$XDG_DATA_HOME/com.netvisionpro.app` 或 `~/.local/share/com.netvisionpro.app` 选择。Linux 的相对 XDG 路径不采用。找不到用户目录时使用当前目录 `.netvision-pro`，不会把历史静默写入共享临时目录。

Linux ICMP 依赖 `ping`（Ubuntu/Debian：`iputils-ping`）；路由追踪依赖 `traceroute`。缺少命令/权限时结果为未知或明确错误，不伪造 0% 丢包。桌面 Wi-Fi 详细信息仍仅 Windows 支持。

源码构建不要求 Node、GTK、WebKit 或显示服务器：

```sh
cargo build --locked --release --manifest-path src-tauri/Cargo.toml --no-default-features --bin netvision-cli
```

## 更新与回退

Windows 桌面继续使用官方 `latest.json`、HTTPS 和内置 minisign 公钥；用户看到的版本必须与重新检查后的安装版本一致，并且同一进程只允许一个安装任务。下载或签名校验失败不执行安装。

CLI 不进行自替换。每次 Release 同时提供架构归档、`.sig` 和 SHA-256。使用官方来源下载、公钥验证工具校验 `.sig`（公钥见 `src-tauri/tauri.conf.json` 的 updater 配置），并校验 `SHA256SUMS.txt` 后，停止外部调度，备份二进制与数据，再替换。可用 Tauri/Rust 的 minisign 兼容验证器；不要把 Base64 公钥字符串直接当作原始 minisign 公钥文件。CLI 版本签名带有受保护的版本注释。

本轮没有声称完成 CLI 自动下载/自替换、原桌面端故障安装回滚或 Apple Gatekeeper 验收。保留旧二进制与数据快照是人工回退方案；恢复旧数据库必须单独明确操作，不能覆盖用户新数据。

## 长期同步要求

共享引擎、协议、结果模型和 SQLite 的修改必须跑 Windows 桌面 CI 与四平台 headless CI。新增桌面 IPC 不强制在 CLI 增加等价交互；修改 CLI 结果/退出码需保留旧脚本兼容。所有平台共用 package/Cargo/Tauri 版本检查，只有全矩阵通过才进入 Release。
