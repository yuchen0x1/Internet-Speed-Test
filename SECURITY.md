# Security policy

安全修复以最新稳定版为目标；旧版本请先检查 Releases。不要在公开 Issue 中提交凭据、网络标识、数据库、私钥或完整未脱敏日志。

请通过 [GitHub 私密漏洞报告](https://github.com/yuchen0x1/Internet-Speed-Test/security/advisories/new) 提交复现条件、影响版本和最小证据。维护者人工评估后协调修复；没有承诺无法保证的响应时限。

## 信任边界

- 测速和系统诊断在本机运行。UI 不直接访问远端网络，导出结果应先脱敏。
- 更新仅使用官方 HTTPS Release，Tauri 在安装前使用内置公钥验证 minisign 签名。SHA-256 校验文件只用于完整性检查，不能独立证明来源。
- 签名私钥只存在于受保护的本机路径或 Actions Secrets；客户端没有仓库写权限 Token。
- Windows Authenticode 与 updater minisign 是两套机制，当前未配置 Authenticode。
- CLI 不自动执行远端二进制；替换版本前校验签名并保留原二进制和数据备份。

## 当前依赖例外（2026-11-18）

构建工具的 picomatch 存在 [GHSA-fc5q-39xc-vjpf](https://github.com/advisories/GHSA-fc5q-39xc-vjpf)，上游当前没有已发布修复。Vite/Vitest/fdir/tinyglobby 的 5 个高危审计节点由这一条公告传播而来。它们不包含在 Windows 安装包或 CLI 中；本项目不接受外部 glob 模式，开发服务器仅绑定 loopback。

`npm run security:check` 只允许这条公告影响 **dev-only** 依赖，例外截至 **2026-12-18**；新公告、生产依赖或过期例外均阻断 CI。原始 npm audit 仍会报告高危，不能把例外描述为漏洞已修复。Dependabot 每周检查，上游修复发布后立即取消例外。
