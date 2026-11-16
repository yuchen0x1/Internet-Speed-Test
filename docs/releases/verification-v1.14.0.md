# v1.14.0 发布验证

验证日期：2026-11-16（北京时间）  
标签：`v1.14.0`  
源码提交：`af55e44aa9edf738fdb02109a5addf4192131e48`

## GitHub Actions

| 工作流 | Run | 结果 |
| --- | --- | --- |
| `CI`（`main`） | [43889957474](https://github.com/yuchen0x1/Internet-Speed-Test/actions/runs/43889957474) | success |
| `CI`（`v1.14.0`） | [43890129454](https://github.com/yuchen0x1/Internet-Speed-Test/actions/runs/43890129454) | success |
| `Release`（`v1.14.0`） | [43890129392](https://github.com/yuchen0x1/Internet-Speed-Test/actions/runs/43890129392) | success |

Release 工作流完成了 Windows NSIS 构建、Tauri updater 签名、产物整理、`SHA256SUMS.txt`、`latest.json` 和 GitHub Release 创建。

## 公开 Release 资产

公开 Release：[v1.14.0](https://github.com/yuchen0x1/Internet-Speed-Test/releases/tag/v1.14.0)

| 资产 | 大小 | 核对结果 |
| --- | ---: | --- |
| `NetVision.Pro_1.14.0_x64-setup.exe` | 4,397,437 bytes | HTTP 200；本地 SHA-256 与 `SHA256SUMS.txt` 一致 |
| `NetVision.Pro_1.14.0_x64-setup.exe.sig` | 448 bytes | 存在；内容与 `latest.json` 的 Windows 签名字段一致 |
| `SHA256SUMS.txt` | 101 bytes | 包含安装包哈希 |
| `latest.json` | 1,341 bytes | `version=1.14.0`，Windows URL 与 Release 安装包一致 |

安装包 SHA-256：

```text
f4efe7fd117babf999b2dba7037a77d0b44382f35742fb59165c725151c32c77
```

## 验证边界

- 已验证 CI、签名构建、公开 Release 资产、更新清单和安装包哈希。
- 没有把 GitHub Actions 成功等同于旧客户端真实发现、验签、安装、重启的完整更新 E2E；该项仍是后续 EV-015。
- Windows Authenticode、屏幕阅读器、高 DPI、24 小时/7 天持久化监控和长期公网运行仍未纳入本次发布验收。
