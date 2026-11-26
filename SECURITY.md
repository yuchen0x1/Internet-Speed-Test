# Security policy

安全修复以最新稳定版为目标；旧版本请先检查 Releases。不要在公开 Issue 中提交凭据、网络标识、数据库、私钥或完整未脱敏日志。

请通过 [GitHub 私密漏洞报告](https://github.com/yuchen0x1/Internet-Speed-Test/security/advisories/new) 提交复现条件、影响版本和最小证据。维护者人工评估后协调修复；没有承诺无法保证的响应时限。

## 信任边界

- 测速和系统诊断在本机运行。UI 不直接访问远端网络，导出结果应先脱敏。
- 更新仅使用官方 HTTPS Release，Tauri 在安装前使用内置公钥验证 minisign 签名。SHA-256 校验文件只用于完整性检查，不能独立证明来源。
- 签名私钥只存在于受保护的本机路径或 Actions Secrets；客户端没有仓库写权限 Token。
- Windows Authenticode 与 updater minisign 是两套机制，当前未配置 Authenticode。
- CLI 不自动执行远端二进制；替换版本前校验签名并保留原二进制和数据备份。

## 依赖审计（v1.15.7）

`npm run security:check` 对 `package-lock.json` 执行 `npm audit`，本轮退出 0，没有已知漏洞。CI、正式发布与补传工作流都执行 `cargo audit`，已知漏洞会阻断；维护性和 unsound 告警完整输出并按实际发布目标复核，不通过 ignore 隐藏。Dependabot 每周检查依赖更新。

本轮 Windows CI 的 Rust 审计退出 0，仍有以下 6 项告警，不能描述为全部依赖没有风险：

| 公告 | 依赖 | 类型与范围 |
|---|---|---|
| [RUSTSEC-2026-0543](https://rustsec.org/advisories/RUSTSEC-2026-0543) | chrono 0.4.45 | 停止维护；是本项目直接运行时依赖，当前公告没有可直接升级的修复版，保留为后续迁移风险。 |
| [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370) | proc-macro-error 1.0.4 | 停止维护；未进入当前 Windows 桌面或 headless CLI 依赖树。 |
| [RUSTSEC-2026-0371](https://rustsec.org/advisories/RUSTSEC-2026-0371) | zbus_names 4.3.5 | 停止维护；来自未发布的 Linux 桌面依赖树。 |
| [RUSTSEC-2026-0370](https://rustsec.org/advisories/RUSTSEC-2026-0370) | zvariant 5.15.1 | 停止维护；来自未发布的 Linux 桌面依赖树。 |
| [RUSTSEC-2026-0372](https://rustsec.org/advisories/RUSTSEC-2026-0372) | zvariant_derive 5.15.1 | 停止维护；来自未发布的 Linux 桌面依赖树。 |
| [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429) | glib 0.18.5 | VariantStrIter 的 unsound 实现；上游修复要求 glib >=0.20，但 Tauri 的 GTK3 依赖仍使用 0.18，不能只在本项目强行跨不兼容版本替换。当前 Windows 桌面和 headless CLI 不包含此包，不发布 Linux 桌面包。 |

`cargo tree --target x86_64-pc-windows-msvc` 与 Linux `--no-default-features` 的依赖树用于核对上述发布范围。若新增 Linux 桌面支持，必须先重新处理这些传递依赖。GitHub Dependabot 和 Secret scanning 在本轮核对时开放告警均为 0；Code scanning 尚未启用，不能声称通过 CodeQL 扫描。审计是时点检查，不保证未来没有新公告。
