# Security policy

安全修复以最新稳定版为目标；旧版本请先检查 Releases。不要在公开 Issue 中提交凭据、网络标识、数据库、私钥或完整未脱敏日志。

请通过 [GitHub 私密漏洞报告](https://github.com/yuchen0x1/Internet-Speed-Test/security/advisories/new) 提交复现条件、影响版本和最小证据。维护者人工评估后协调修复；没有承诺无法保证的响应时限。

## 信任边界

- 测速和系统诊断在本机运行。UI 不直接访问远端网络，导出结果应先脱敏。
- 更新仅使用官方 HTTPS Release，Tauri 在安装前使用内置公钥验证 minisign 签名。SHA-256 校验文件只用于完整性检查，不能独立证明来源。
- 签名私钥只存在于受保护的本机路径或 Actions Secrets；客户端没有仓库写权限 Token。
- Windows Authenticode 与 updater minisign 是两套机制，当前未配置 Authenticode。
- CLI 不自动执行远端二进制；替换版本前校验签名并保留原二进制和数据备份。

## 依赖审计（2026-11-26）

`npm run security:check` 对 `package-lock.json` 执行 `npm audit`，当前没有已知漏洞。CI 对任何新公告均默认阻断；如确需短期例外，必须在本文记录公告编号、影响范围、缓解措施和到期日，并在审计脚本中显式限定，不能把例外描述为漏洞已修复。Dependabot 每周检查依赖更新。
