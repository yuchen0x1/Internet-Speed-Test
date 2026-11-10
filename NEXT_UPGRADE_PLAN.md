# Next Upgrade Plan

**更新时间：** 2026-11-10  
**当前基线：** NetVision Pro v1.9.0；当前工作区更改尚未提交或发布。

## P0 · 本地工作区收尾

1. 审阅 `git diff`，区分用户既有改动、本轮代码与 `cargo fmt` 格式化差异；不得用 checkout/reset 覆盖。
2. 在具备 registry 访问的环境安装并运行 `cargo audit --deny warnings --file src-tauri/Cargo.lock`；按真实 advisory 修复，不掩盖警告。
3. 审阅 `.github/workflows/ci.yml` 的版本 pin/action 引用与运行结果；当前 workflow 尚未由 GitHub 执行。可用 Dependabot 定期更新 action 与依赖，并评估 SHA pinning。
4. 人工运行 Windows Tauri 应用，验收 Modal 焦点、快捷键、历史加载更多、诊断类型筛选、监控暂停状态恢复和设置路径显示。
5. 更新完整项目分析、开源研究和最终报告；保留用户截图和未确认用途的临时目录。

## P1 · 工程化和验证

- 建立可重复 Windows 桌面 smoke/E2E 验收，覆盖页面导航、测速取消、设置持久化、历史操作和键盘可达性；隔离网络依赖。
- 对依赖升级分组执行：先 Cargo patch 更新，再验证 Windows build/tests/audit；Vite 6 → 7 与 React 插件单独升级并对照 Node peer/engine 要求。
- RustSec 与 npm advisory 使用官方数据库；记录检查日期、命令和未访问网络的限制。
- 评估 Windows 代码签名/自动更新时，先定义密钥保管、签名验证、回滚和发布授权；不得自动上传 Release。

## P2 · 可选功能（先做设计）

- PMTU 探测：确定探测包、超时、平台 API、是否需要管理员权限与失败呈现后再实现。
- Wi-Fi 信道扫描：仅读取 Windows WLAN 信息，明确隐私字段与系统语言/权限影响；先做 fixture 和 mock 测试。
- 诊断协议模块化按增量需求进行，避免纯结构性大重写。

## 不执行

- 未经明确要求发布正式版本、推送代码或上传项目。
- 不删除用户截图、工作区文件、用户数据或未经检查的临时目录。
- 不因 CI 失败关闭 lint/audit/测试来“变绿”。
- 不接入违反测速服务 ToS 的私有接口，不复制完整开源项目。
