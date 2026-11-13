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

## 发布与资产核对

发布源：[v1.13.0](https://github.com/yuchen0x1/Internet-Speed-Test/releases/tag/v1.13.0)。本轮按现有 Release 工作流发布 Windows x64 安装包、`.exe.sig`、`latest.json` 与 `SHA256SUMS.txt`，发布后另行核对远端实物。

## 验收边界

- minisign 更新包签名与 Windows Authenticode 证书用途不同；本项目仍未配置后者。
- 不覆盖向正式安装目录执行跨版本覆盖安装、自动重启、长期运行、满磁盘和全部公网诊断。
- 发布资产与签名验证结果、远端 CI 状态在本轮完成后补充。
