# Security policy

安全修复以最新稳定版为目标；旧版本请先检查 Releases。不要在公开 Issue 中提交凭据、网络标识、数据库、私钥或完整未脱敏日志。

请通过 [GitHub 私密漏洞报告](https://github.com/yuchen0x1/Internet-Speed-Test/security/advisories/new) 提交复现条件、影响版本和最小证据。维护者人工评估后协调修复；没有承诺无法保证的响应时限。

## 信任边界

- 测速和系统诊断在本机运行。UI 不直接访问远端网络，导出结果应先脱敏。
- 更新仅使用官方 HTTPS Release，Tauri 在安装前使用内置公钥验证 minisign 签名。SHA-256 校验文件只用于完整性检查，不能独立证明来源。
- 签名私钥只存在于受保护的本机路径或 Actions Secrets；客户端没有仓库写权限 Token。
- Windows Authenticode 与 updater minisign 是两套机制，当前未配置 Authenticode。
- CLI 不自动执行远端二进制；替换版本前校验签名并保留原二进制和数据备份。

## 依赖审计（v1.15.8）

`npm run security:check` 对 `package-lock.json` 执行官方源 `npm audit`。CI 和正式发布对 Rust 执行 `cargo audit --deny warnings`，已知漏洞、停止维护和 unsound 告警均会阻断。本地第三方补丁另由 `node scripts/check-dependency-patches.mjs` 检查实际解析来源和源码完整性，不能只依赖审计退出码。没有配置 RustSec ignore，也不修改公告数据库或伪造依赖版本号。Dependabot 每周检查依赖更新。

2026-11-27 本地严格审计检查 544 个依赖，退出 0，未报告漏洞或告警；npm 审计同样通过。该结果包含本轮锁文件与补丁状态，不代表所有第三方源码已经没有缺陷，也不能替代下述构建、回归测试与代码扫描。

上一版遗留的 6 项告警分别处理如下：

| 公告 | 原依赖 | 本版代码处理 |
|---|---|---|
| [RUSTSEC-2026-0543](https://rustsec.org/advisories/RUSTSEC-2026-0543) | chrono 0.4.45 | 应用时间处理迁移到 Jiff；保留旧历史格式和本地时区规则。serde_with 的未启用 chrono 集成从本地补丁中移除，误启用会明确编译失败，锁文件不再解析 chrono。 |
| [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370) | proc-macro-error 1.0.4 | GTK/glib 宏库直接使用 syn 的错误返回与编译诊断，移除停止维护的宏错误库；同样阻止已停止维护的 proc-macro-error2 回流。 |
| [RUSTSEC-2026-0371](https://rustsec.org/advisories/RUSTSEC-2026-0371) | zbus_names 4.3.5 | 通知与单实例插件迁移到 zbus 6 的 names API，移除独立旧包。 |
| [RUSTSEC-2026-0370](https://rustsec.org/advisories/RUSTSEC-2026-0370) | zvariant 5.15.1 | 通知信号及 D-Bus 值使用 zbus 6 的 wire API；notify-rust 使用兼容该 API 的版本，移除旧包。 |
| [RUSTSEC-2026-0372](https://rustsec.org/advisories/RUSTSEC-2026-0372) | zvariant_derive 5.15.1 | 使用 zbus 6 提供的派生实现，移除旧宏包。 |
| [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429) | glib 0.18.5 | 对兼容 Tauri GTK3 的本地源码回移官方 [gtk-rs-core#1343](https://github.com/gtk-rs/gtk-rs-core/pull/1343) 补丁，将 C 输出参数改为可变指针；新增优化构建下 next/nth/last/反向迭代回归。没有把它伪装成官方 0.20 升级。 |

## 本地补丁与验证

第三方源码的官方归档、校验和、许可证、修改范围和后续移除条件见 [vendor 维护说明](vendor/README.md)。保留上游真实版本与原始 manifest，补丁通过根项目 `[patch.crates-io]` 显式选用。路径依赖没有 registry 来源，RustSec/Dependabot 的版本检查不能单独证明补丁有效；独立检查器固定审阅过的源码摘要，验证 glib 实际修复和回归测试存在、锁文件没有退回 registry 版本，并阻断已移除包回流。

新增 Linux 桌面依赖编译、glib 优化回归、通知信号与单实例 D-Bus 测试 CI，验证此前未被 Windows 构建覆盖的路径；不因此声称已经提供 Linux 桌面发行包。Windows 桌面和四平台 CLI 继续使用实际编译、测试与 Clippy 门禁。本机缺少 MSVC 链接器，不能用本地格式检查代替这些 CI。

新增 JavaScript/TypeScript 与 Rust CodeQL 工作流。是否执行成功及实际告警应查对应提交的 Actions/Security 结果，配置工作流不等于扫描已经通过。Secret scanning 和推送保护保持启用；历史 dismissed 告警不等于已修复，需结合源码补丁和后续依赖图重新核查。审计是时点检查，不保证未来没有新公告。
