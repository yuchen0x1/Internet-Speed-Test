# NetVision Pro 全面分析与升级方案

**分析日期**：2026-11-10
**基线版本**：v1.9.0（HEAD `e2d3c19`，工作区含 8 个已修改文件 + 10 个未跟踪文件）
**方法**：静态代码走查 + 依赖版本核对 + 本机实跑验证（未启动 GUI、未发布、未提交）
**配套文档**：[项目审计报告](../PROJECT_AUDIT_REPORT.md)、[路线图](../ROADMAP.md)、[对标分析](../GITHUB_BENCHMARK_REPORT.md)

---

## 1. 核查基线（本次实际运行结果）

| 检查项 | 结果 |
| --- | --- |
| `npm test`（vitest 5.1.0） | ✅ 5 文件 / 33 测试通过，3.67s |
| `npm run typecheck`（tsc 5.9.3） | ✅ 通过 |
| `npm run build`（vite 6.4.5） | ✅ 373.73 kB JS（gzip 112.89 kB）、34.56 kB CSS（gzip 7.90 kB） |
| `cargo test --locked` | ✅ 56 passed / 0 failed / 3 ignored |
| `cargo check` | ⚠️ 通过，9 条 dead-code 警告 |
| `cargo clippy --all-targets` | ⚠️ 31 条警告（0 error） |
| `cargo fmt --check` | ❌ 失败，86 处差异，涉及 17 个文件 |
| `npm audit`（官方 registry） | ✅ 0 vulnerabilities |
| 节点发现端点连通性 | ✅ librespeed 200 / Cloudflare 200 / M-Lab Locate 200 |
| 锁文件 | ✅ `Cargo.lock`、`package-lock.json` 均未被本次分析改动 |

**代码规模**：前端 + 样式 7064 行，Rust 8131 行，合计约 14k 行。8 个页面、47 个 Tauri 命令、48 个前端 IPC 封装。

---

## 2. 结论摘要

工程骨架是健康的：分层清晰（React/Zustand → IPC → Rust 引擎/诊断/存储）、真实测量无造假、NDT7 令牌边界处理到位、错误路径大多有诚实降级。

但存在一类**更值得优先处理的问题：功能承诺与实际行为不一致**。多个设置项在 UI 和文档里存在，Rust 侧根本没有任何消费者；历史页的统计口径实际只覆盖最近 500 条却按"全部"呈现。这类问题比"缺功能"更伤害专业产品的可信度。

技术债集中在三处：**格式化/lint 未纳入 CI 门禁**（fmt 已 86 处漂移）、**`panic = "abort"` 叠加 51 处非测试 `unwrap`**、**release 缺少签名与自动更新链路**。

| 维度 | 评分 | 判断依据 |
| --- | ---: | --- |
| 功能完整度 | 84 | 测速/诊断/监控/历史/批测/CLI/Prometheus 覆盖面广 |
| 代码质量 | 76 | 严格 TS + 56 个 Rust 测试；fmt 漂移、31 条 clippy、9 条 dead-code |
| 可靠性 | 70 | 51 处非测试 unwrap + `panic = "abort"`；设置写入有崩溃窗口 |
| 数据准确性 | 68 | 历史统计/清空提示按 500 条窗口计算，与"全部"表述不符 |
| 安全性 | 79 | URL 白名单、CSP、loopback、令牌脱敏做得好；CSV 导出未全字段转义 |
| 交付成熟度 | 62 | 无签名、无自动更新、无 release 自动化、无 UI E2E |
| 可访问性 | 45 | 0 条 `:focus-visible`；模态框无焦点陷阱/`role="dialog"` |
| 工程化 | 65 | CI 只跑 test/build，无 lint/audit 门禁，无工具链版本固定 |

---

## 3. 已确认缺陷（按严重度）

### 3.1 P0 — 数据可信度与正确性

**① 历史统计与「清空全部」按 500 条窗口计算，却按全量表述**

- `src/pages/HistoryPage.tsx:46` 固定 `limit: 500`。
- 平均下载/最佳下载/日均对比/热力图/趋势图全部基于 `rows`（≤500 条）计算。
- `src/pages/HistoryPage.tsx:308` 确认框文案为「将永久删除全部 `${rows.length}` 条」，当库内 >500 条时数字错误。
- `src-tauri/src/storage.rs:303` 已有 `history_count()`，但**从未通过 IPC 暴露**，前端无从获知真实总数。

影响：长期用户看到的"平均下载速度"会静默偏离真实均值，删除确认的条数也不对。**修复成本低（约 0.5 天），收益最高。**

**② CSV 导出字段未转义 + 无公式注入防护**

- `src-tauri/src/commands.rs:497-498`：`public_ip`、`isp` 直接 `unwrap_or_default()` 拼接，未经 `csv_escape`。
- `created_at`、`status` 同样未转义。
- `server_name` 来自远端 `librespeed.org` 的服务器列表（`selector.rs:22` 在线拉取 + 内置快照），属远程可控字符串；未对 `= + - @` 前缀做中和。

影响：ISP 名含逗号（如 `China Mobile Communications Group Co., Ltd.`）会直接错列；恶意/被污染的节点列表可向用户 Excel 注入公式。**约 0.5 天。**

**③ 设置写入存在崩溃窗口（原子性名不副实）**

`src-tauri/src/settings.rs:151-152`：

```rust
let _ = tokio::fs::remove_file(&self.path).await;   // 先删
tokio::fs::rename(&tmp, &self.path)                  // 再改
```

若在这两步之间进程被杀/断电，`settings.json` 已消失、只剩 `.json.tmp`，下次启动按"首次运行"重置为默认值——用户全部设置静默丢失，且不会走 `.bak` 恢复路径（`.bak` 只在解析失败时产生）。

本次已用真实 Rust 程序验证：**Windows 上 `std::fs::rename` 对已存在的目标文件会直接覆盖**（输出 `rename overwrote: new`），因此 `remove_file` 这一步既无必要又制造了风险。修复即删除该行。**约 0.2 天。**

**④ Prometheus 端口绑定失败时 UI 仍显示「已开启」**

- `src-tauri/src/prometheus.rs:100-112`：绑定失败仅 `log::warn!` 并回滚内部状态。
- `src/pages/SettingsPage.tsx:293-295`：只要 `prometheus_enabled` 为真就渲染 `http://127.0.0.1:{port}/metrics`。

影响：端口被占用时用户看到"已开启"却抓不到任何指标。需新增 `prometheus_status` 命令（running / 错误原因），前端据此显示真实状态。**约 0.5 天。**

### 3.2 P1 — 虚标功能（文档/UI 承诺但无实现）

| 设置项 | 声明位置 | 实际消费者 | 结论 |
| --- | --- | --- | --- |
| `save_diag_history` | `settings.rs:70`、`SettingsPage.tsx:333`、`docs/configuration.md:47`「诊断结果保存在本机会话」 | **0 处**（Rust 与前端均只读写不消费，也没有任何诊断历史表） | 完全虚标 |
| `refresh_interval_ms` | `settings.rs:22`、`SettingsPage.tsx:85`「数据刷新频率」 | **0 处**（`monitor.rs:120` 硬编码 1000ms） | 完全失效 |

这两项属于"专业软件最不该有"的问题。两种处理路径，建议分开对待：

- `refresh_interval_ms`：**接通**。monitor tick 循环每轮读一次 `RwLock` 即可，约 0.3 天，且能让 1h 缓冲的点数随之自适应。
- `save_diag_history`：**实现**。storage 已有版本化迁移骨架，加 `diag_runs` 表 + 诊断页"历史"抽屉，约 1.5 天。若本迭代不做，则必须同时从设置页、`settings.rs`、配置文件文档三处移除，避免继续虚标。

**⑤ 测试从托盘/重载后前端状态失同步**

- `commands.rs:294` `speedtest_running` 与 `engine/mod.rs:111` `current_phase` 都存在，前端 `api.ts:78` 也封装了 `speedtestRunning`，但**全项目 0 处调用**。
- 托盘菜单可直接启动测速（`lib.rs:113-122`）。若此时 UI 停留在其他页面，或测速中重载窗口，前端 `useTest` store 的 `running/phase` 会持续为 idle，直到引擎自行完成。

修复即调用已有命令做一次启动/聚焦时的状态对账，**约 0.5 天**，同时让其中 2 条 dead-code 警告自然消失。

### 3.3 P2 — 健壮性与可维护性

**⑥ `panic = "abort"` 叠加 51 处非测试 `unwrap`**

release profile（`Cargo.toml:44`）为 `panic = "abort"`，而非测试代码中 `unwrap/expect` 分布：

| 文件 | 数量 |
| --- | ---: |
| `engine/mod.rs` | 12 |
| `storage.rs` | 9 |
| `engine/quality.rs` | 6 |
| `pingmon.rs` | 5 |
| `monitor.rs` / `ndt7.rs` / `batchtest.rs` | 各 4 |

其中绝大多数是 `Mutex::lock().unwrap()`。一旦某线程 panic 导致互斥量中毒，后续任意访问都会 panic；在 `abort` 模式下这意味着**整个应用静默退出**（托盘图标消失、无提示、无日志上报路径）。

两条可选路线：
- **A（推荐）**：对锁用 `lock().unwrap_or_else(|e| e.into_inner())`——中毒后继续使用数据，符合监控/计数这类"数据仍可用"的语义；保留 `abort` 以获得更小体积。
- **B**：release 改 `panic = "unwind"` + 顶层 `catch_unwind`，代价是二进制增大、需要审慎处理跨 FFI 边界。

**⑦ React 无错误边界、无全局错误处理**

全项目 0 处 `ErrorBoundary` / `componentDidCatch` / `window.onerror` / `unhandledrejection`。任一页面渲染抛错将整棵树卸载为白屏（`App.tsx` 无 fallback）。桌面应用这一点比 Web 更严重——用户没有"刷新页面"的直觉。**约 0.5 天**：加 `ErrorBoundary` 包裹 `<main>`、加全局 handler 写日志并 Toast。

**⑧ NDT7 发现结果进程级永久缓存，无失效路径**

`ndt7.rs:63-67`：首次成功发现后写入 `cached`，此后不再刷新。若该社区节点后续下线，自动优选会持续指向死节点，直到重启应用。建议加 TTL（如 10 分钟）或"测速失败时清缓存重试一次"。**约 0.3 天。**

**⑨ `run_test` / `execute` 过长，扩展点拥挤**

`engine/mod.rs` 中 `execute()` 约 201 行，`run_throughput_phase()` 带 `#[allow(clippy::too_many_arguments)]`（8 参数，`mod.rs:481`）。ROADMAP「技术债 #5」提出抽取 `PhaseRunner`，至今未做。再叠加"计划测速 / 多窗口并行"等规划项前，此重构应先行。

**⑩ 文档版本漂移**

| 位置 | 现状 |
| --- | --- |
| `docs/usage.md:3` | 「本文对应 v1.8.0」 |
| `docs/configuration.md:25` | 「以下值来自 v1.8.0 的 Rust 默认配置」 |
| `README.md:191` | 变更日志止于 v1.8.0，无 v1.9.0 段落 |
| `Sidebar.tsx:37`、`SettingsPage.tsx:384` | 兜底字符串仍为 `"1.8.0"` |
| `docs/screenshots/` | 8 张未跟踪截图未被任何文档引用 |

---

## 4. 依赖与工具链现状

### 前端（npm）

| 包 | 当前 | Latest | 建议 |
| --- | --- | --- | --- |
| `vite` | 6.4.5 | **8.3.7** | 分两步：先升 **7.3.8**（低风险），vite 8 待生态稳定 |
| `@vitejs/plugin-react` | 4.7.0 | **6.3.0** | 6.x 仅支持 vite ^8，且新增 `oxc-transform-react`/`@rolldown/plugin-babel` peer。配 vite 7 应用 **5.2.0** |
| `typescript` | 5.9.3 | **7.0.2** | **暂缓**。TS 7 为原生移植版，大规模行为差异，建议等 7.1+ 并在独立分支验证 |
| `png-to-ico` | 2.1.8 | **3.0.2** | 仅图标构建脚本使用，可随下次图标更新一并升 |
| `react` / `react-dom` | 19.3.1 | 19.3.1 | ✅ 已最新 |
| `zustand` | 5.1.0 | 5.1.0 | ✅ 已最新 |
| `jsdom` | 31.0.0 | 31.0.0 | ✅ 已最新 |
| `sharp` | 0.35.5 | 0.35.5 | ✅ 已最新 |
| `vitest` | 5.1.0 | 5.1.0 | ✅ 已最新；peer 已支持 vite ^6.4/^7/^8 |

### Rust（cargo）

| 包 | 当前 | 说明 |
| --- | --- | --- |
| `tauri` | 2.12.5 | ✅ 2.x 最新稳定。**3.0 仍为 alpha.6，不要采用** |
| `tauri-plugin-updater` | 未引入 | 稳定版 **2.14.1** 可用（3.0 为 alpha） |
| `tauri-driver` | 未引入 | 稳定版 **2.1.0** 可用于 E2E |
| `reqwest` | 0.13.5 | 已是最新 major，注意无 `gzip` feature（响应不自动解压） |
| `cargo update --dry-run` | — | 12 个补丁级可升：`brotli-decompressor`、`jiff`、`notify-rust`、`syn`、`uuid` 等；`zbus` 系列仅在 Linux 目标引入 |

**工具链**：Rust 1.99.1 ✅（tauri 2.x 要求 1.90+）；Node 24.20 本地 / CI 用 22 → **建议加 `rust-toolchain.toml` 与 `.nvmrc`（或 `package.json#engines`）固定，消除本地与 CI 漂移。**

---

## 5. 工程化与发布差距

| 缺失项 | 现状 | 影响 |
| --- | --- | --- |
| CI lint 门禁 | `.github/workflows/ci.yml` 仅 typecheck / test / build / cargo test | fmt 已漂移 86 处却无人拦截 |
| CI 依赖审计 | 无 `cargo audit` / `cargo deny` / `npm audit` | 与本地"0 漏洞"结论脱节 |
| Release 自动化 | 仅 `ci.yml`；v1.9.0 是手工打 tag + 构建 | 无产物签名、无校验和自动生成、无 `latest.json` |
| 代码签名 | `tauri.conf.json` 无 `certificateThumbprint`/`signCommand` | 每次安装触发 SmartScreen 警告 |
| 自动更新 | 未引入 | 用户需手动下载新版本 |
| UI E2E | 无 | 8 个页面的交互链路无自动化回归 |
| `cargo fmt` 一致性 | 86 处差异 / 17 文件 | 任何后续 PR 都会夹带格式噪音 |
| `.nvwork/check.py` | 已跟踪的临时脚本 | 仓库卫生；建议移入 `scripts/` 或删除 |

**关键澄清**：更新器签名（minisign）与 Authenticode 代码签名是两件独立的事。前者只保证更新包在传输中未被篡改，**不能消除 SmartScreen 警告**。要消警告仍需 OV/EV 证书（年费成本，需用户决策）。

---

## 6. 升级方案

### P0 · 数据可信度修复（约 2 天，建议 v1.9.1 发版）

| # | 动作 | 落点 | 验收 |
| --- | --- | --- | --- |
| 1 | 暴露 `history_count` IPC；历史页显示「共 N 条 / 已加载 M 条」；统计卡片与清空确认按真实总数计算 | `storage.rs`、`commands.rs`、`api.ts`、`HistoryPage.tsx` | 库内 >500 条时，文案与均值口径一致 |
| 2 | CSV 全字段过 `csv_escape`；对 `= + - @` 起始值加 `'` 前缀 | `commands.rs::to_csv` | 新增含逗号 ISP 与公式前缀的用例 |
| 3 | 删除 `remove_file` 前导步骤，直接 rename 覆盖 | `settings.rs:151` | 断电模拟后设置可恢复 |
| 4 | 新增 `prometheus_status` 命令并在设置页显示真实状态 | `prometheus.rs`、`commands.rs`、`SettingsPage.tsx` | 占用端口时显示"绑定失败: <原因>" |

### P1 · 消除虚标 + 状态一致性（约 3 天）

| # | 动作 | 落点 | 说明 |
| --- | --- | --- | --- |
| 5 | `refresh_interval_ms` 接通 monitor tick | `monitor.rs:120` | 每轮读一次设置，clamp 200–5000ms |
| 6 | `save_diag_history` 落地：`diag_runs` 表（v2 迁移）+ 诊断页历史抽屉 | `storage.rs`、`diagnostics.rs`、`DiagnosticsPage.tsx` | 若砍掉，则三处同步移除 |
| 7 | 启动/窗口聚焦时用 `speedtest_running` + `current_phase` 对账 | `App.tsx`、`store/test.ts` | 托盘启动的测速在 UI 上可见 |
| 8 | 加 `ErrorBoundary` + `window.onerror`/`unhandledrejection` → 日志 + Toast | 新建 `components/ErrorBoundary.tsx` | 单页抛错不白屏 |
| 9 | NDT7 发现缓存加 TTL / 失败清缓存 | `ndt7.rs:63` | 节点下线后可自愈 |

### P1 · 交付链路（约 3 天，不含证书采购）

| # | 动作 | 说明 |
| --- | --- | --- |
| 10 | CI 增加 `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo audit`、`npm audit` | 先把现有 31 条 clippy 与 86 处 fmt 一次性清账，再开门禁 |
| 11 | 新增 `.github/workflows/release.yml`：tag 触发 → 构建 NSIS → 生成 `.sig` 与 `latest.json` → 上传 Release | 需在仓库 Secrets 配 `TAURI_SIGNING_PRIVATE_KEY` |
| 12 | 接入 `tauri-plugin-updater@2.14.1` + 设置页"检查更新" | 前置：第 11 项 |
| 13 | 固定工具链：`rust-toolchain.toml`（1.99.1）、CI `node-version: 24` | 消除本地/CI 漂移 |
| 14 | 代码签名证书（**需用户决策**：OV/EV 年费） | 唯一能消除 SmartScreen 的路径 |

### P2 · 质量提升（约 4 天）

| # | 动作 | 说明 |
| --- | --- | --- |
| 15 | **重排权限**：把 `Cargo.toml` 的 `panic` 保持 `abort`，锁访问改 `unwrap_or_else(|e| e.into_inner())`（12 处 `engine/mod.rs` + 9 处 `storage.rs` 优先） | 消除"中毒锁 → 应用静默退出" |
| 16 | 可访问性：全局 `:focus-visible` 规则；模态框补 `role="dialog"`、`aria-modal`、焦点陷阱与初始焦点；侧栏按钮加键盘导航 | 当前 0 条 focus 规则，是专业感最直观的短板 |
| 17 | 8 个页面 `React.lazy` 拆包 | 373 kB 单包 → 首屏显著下降；现有 `key={page}` 重挂载结构天然适配 |
| 18 | 抽取 `PhaseRunner`，拆解 `execute()` 201 行 | 为"计划测速增强 / 多窗口并行"铺路 |
| 19 | 诊断深度：PMTU 探测（DF 逐级试探）、Wi-Fi 邻居信道扫描 | ROADMAP 遗留项，当前 0 实现 |
| 20 | 存储升级：samples 独立表 + FTS5（ROADMAP 技术债） | 数千条历史不膨胀 |
| 21 | 文档同步：usage/configuration/README changelog 升至 v1.9.0，引用或清理未跟踪截图 | 消除版本漂移 |

### P3 · 依赖升级（约 1.5 天，建议 P0–P2 稳定后单独 PR）

| 阶段 | 动作 | 风险 |
| --- | --- | --- |
| 3-1 | `cargo update` 12 个补丁级升级 → 全量测试 | 低 |
| 3-2 | `vite 6.4.5 → 7.3.8` + `@vitejs/plugin-react 5.2.0` | 中低，需验证 Tauri dev URL 与构建产物 |
| 3-3 | `png-to-ico 3.0.2`、`sharp` 保持 | 低 |
| 3-4 | `vite 8` + `plugin-react 6.x`（rolldown/oxc） | **暂缓**，等 rolldown 生态与 Tauri 官方模板同时跟进 |
| 3-5 | `typescript 7.0.2` | **暂缓**，原生移植版需独立验证分支 |
| 3-6 | Tauri 3 | **不采用**（alpha.6） |

### P3 · 长期（需用户决策后启动）

- i18n：910 处中文文案分布在 8 个页面；真正的成本在 Rust 侧硬编码中文错误串（需先引入消息键层）。建议先做键抽取，翻译后置。
- 数据云同步 / 分享短链：涉及账号体系与隐私边界，暂无必要。
- UI E2E：`tauri-driver 2.1.0` + GitHub Actions Windows runner，覆盖"启动 → 8 页遍历 → 开始测速 → 断言字段 → 取消"。

---

## 7. 建议排期

**Sprint 1（约 5 天）→ 发 v1.9.1**
P0 全部 4 项 + P1 的 5/7/8 项。目标是"数据可信、状态诚实"，不引入新功能，回归面最小。

**Sprint 2（约 6 天）→ 发 v1.10.0**
P1 的 6/9 项（诊断历史 + NDT7 自愈）+ 交付链路 10–13。这一版解决"能不能长期用下去"：更新、审计、lint 门禁到位。

**Sprint 3（约 4 天）**
P2 全部。重点是可访问性与错误边界，这两项直接影响用户对"专业级"的判断。

**Sprint 4**
P3 依赖升级，单独 PR、单独验证，不与功能混提。

---

## 8. 明确不建议做的事

| 方向 | 理由 |
| --- | --- |
| 升级到 Tauri 3 alpha | alpha.6，插件生态（updater/store）同名插件才 alpha.3，风险不可控 |
| 立即上 TypeScript 7 | 原生移植版重大变更，与当前"稳定交付"目标冲突 |
| 立即上 Vite 8 + plugin-react 6 | 依赖 rolldown/oxc 新 peer 链，收益仅是构建速度，非用户可见 |
| 逆向接入 Ookla / FAST | 违反对方 ToS；现有"官网入口"方案已合规 |
| 为消除警告而删扩展点 | `current_phase`/`speedtest_running`/`monitor_latest` 正是第 7 项所需，应接通而非删除 |
| 整体重写 UI 层 | 现有 8 页结构稳定，重写风险远大于逐项修复收益 |

---

## 9. 验收清单（每个 Sprint 收尾必跑）

```bash
npm run typecheck
npm test
npm run build
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo clippy --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo audit --manifest-path src-tauri/Cargo.toml
npm audit --registry=https://registry.npmjs.org
```

Sprint 1 之后 `fmt` 与 `clippy` 两项应从"预期失败"转为"必须通过"；在此之前不要把它们加进 CI，否则会阻断正常提交。

---

## 10. 本次分析未做的事

- 未启动 GUI、未构建安装包、未做真实网络端到端测速（以上三项均依赖实际运行环境，属历史验证报告范畴）。
- 未执行 `cargo audit`（本机未安装该子命令，且不愿为一次性检查改动环境）。
- 未做跨 DPI / 跨语言 / 跨 Windows 版本实测。
- 未修改任何业务代码，未提交、未推送、未发布。工作区保持分析前状态。
