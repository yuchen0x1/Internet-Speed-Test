# v1.13.0 发布验证记录

目标：Windows 10/11 x64 桌面版。正式设置、数据库和系统安装不参与本轮验证。

## 本地验证

| 检查 | 结果 | 证据 |
| --- | --- | --- |
| `npm run docs:check` | PASS | 14 个版本、镜像与本地链接通过 |
| `npm run typecheck` / `npm test` / `npm run build` | PASS | 67 项前端测试，类型与生产构建退出 0 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | PASS | 退出 0 |
| `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | PASS | 严格 lint 退出 0 |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml` | PASS | 77 通过、4 个真实网络用例默认忽略 |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml loopback_uses_real_icmp_pipeline -- --ignored --nocapture` | PASS | 单独运行真实 Windows 回环 ICMP，1 项通过 |
| `npm audit --registry=https://registry.npmjs.org --json --fetch-timeout=15000 --fetch-retries=0` | PASS | 当前漏洞计数 0 |
| `cargo audit --file src-tauri/Cargo.lock --json` | PASS（保留告警） | 阻断漏洞 0；4 项 unmaintained、1 项 unsound 告警 |

Rust 告警对应 RUSTSEC-2024-0370、RUSTSEC-2026-0371、RUSTSEC-2026-0370、RUSTSEC-2026-0372、RUSTSEC-2024-0429。本轮未修改依赖版本，也未用忽略项消除告警。

开发阶段的原生窗口、切页、停止、重载和 CSV/JSON 实际保存证据见 [开发验收记录](../evolution-batch-metrics-2026-11-13.md)，这些场景不代表完整跨版本安装已经验证。

## 本地签名构建

- PASS：`npm run build:release` 生成 Windows x64 NSIS 安装包与 `.exe.sig`，版本为 1.13.0。
- PASS：使用 updater 相同的 minisign-verify 0.3.0 库验证安装包与客户端公钥，确认签名受保护版本是 1.13.0；修改文件最后一个字节会导致验签失败。
- 本地安装包 SHA-256：`fb39900deb4125f108cc925b69815b6ecfc4f80f8f6ea5391cc831f44e927fe3`。GitHub runner 会独立重建，远端包的校验值另行核对，不能假定两次构建字节相同。
- Authenticode：主程序和 NSIS 包均为 NotSigned，保留已知限制。

## 发布与资产核对

- 正式发布：2026-11-13 20:11:52（北京时间）；[Release](https://github.com/yuchen0x1/Internet-Speed-Test/releases/tag/v1.13.0)。
- 标签源码：`4bce7136b7cc142ff8f6b039c4d184e510aa41e4`，与 Release 工作流的 head SHA 一致。
- PASS：[main CI](https://github.com/yuchen0x1/Internet-Speed-Test/actions/runs/43421882767)、[标签 CI](https://github.com/yuchen0x1/Internet-Speed-Test/actions/runs/43421895896)、[Release 工作流](https://github.com/yuchen0x1/Internet-Speed-Test/actions/runs/43421895890) 均成功。
- PASS：实际下载四个公开附件，逐一核对 GitHub SHA-256 digest 和文件大小；未发布其他平台包、PDB、私有配置或缓存。
- 安装包：`NetVision.Pro_1.13.0_x64-setup.exe`，4,356,104 字节；SHA-256：`694547797b64e1480f85ba09c953605a5237029324573fc162515981aea5fc44`。
- PASS：`SHA256SUMS.txt` 与下载文件一致（兼容 GNU 校验清单的二进制标记 `*`），`.sig` 与 `latest.json` 中的签名一致。
- PASS：对实际远端安装包执行 minisign 验证，与内置公钥匹配、受保护版本为 1.13.0；篡改单字节会被拒绝。
- PASS：匿名 HTTPS 访问正式 `releases/latest/download/latest.json`，返回版本 1.13.0，平台只有 `windows-x86_64`，安装包 URL 指向本版正式附件。

## 验收边界

- minisign 更新包签名与 Windows Authenticode 证书用途不同；本项目仍未配置后者。
- 不覆盖向正式安装目录执行跨版本覆盖安装、自动重启、长期运行、满磁盘和全部公网诊断。
- 稳定端点与签名验证只证明发布包来源、完整性和版本绑定，不代表旧客户端覆盖安装及重启已经实测。
