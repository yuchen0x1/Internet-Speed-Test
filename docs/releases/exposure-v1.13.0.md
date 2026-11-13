# v1.13.0 发布产物暴露面复核

## 1. 总览

本轮按 `reverse-engineering-exposure-audit` 技能扫描签名构建对应的实际 `dist/` 与 Windows 主程序，并复核待提交范围。web 自动评分 100/A（10 个文件，无发现），native 自动评分 69/C（1 个 PE）。评分仅作启发式排序，不是安全认证；原始报告保留在本地忽略目录，不上传二进制字符串或构建机路径。

## 2. 发现清单

| 编号 | 严重度与人工判断 | 证据 | 分析路径 |
| --- | --- | --- | --- |
| PE-SIGN | 高，保留已知限制 | PowerShell 检查主程序及 NSIS 包均为 NotSigned；没有 Authenticode 证书 | PE 证书目录/系统验签 |
| LEAK-INTRANET | 工具报高，人工排除个人网络泄露 | 固定回环 127.0.0.1，用于开发资源和本机指标，不是用户网络清单 | 字符串提取 |
| PE-PDB | 工具报中，人工降为信息 | CodeView 仅记录 `netvision_pro.pdb` 文件名；PDB 文件不分发 | PE 调试目录 |
| PE-CFG | 低，保留 | 未检测到 Control Flow Guard 标志，未等同于已证实的可利用漏洞 | PE load config |
| WEB-SOURCEMAP / SECRET | 未命中 | dist 无源码映射/凭证标记；候选提交无私钥/令牌模式或数据库/缓存文件 | 静态扫描与提交范围检查 |

## 3. 逐项证据与措施

- 前端沿用 `sourcemap: false`，Rust release 使用 `strip = true`。
- 仅分发 Windows 安装包、minisign 签名、更新清单及 SHA-256 清单；排除数据库、设置、密钥、密码、PDB、缓存及原始测试输出。
- 客户端公钥与 `requireSignedVersion` 保留，签名私钥仅由既有受保护目录或 GitHub Secrets 提供。
- PASS：使用锁定的 minisign-verify 库验证本地实际 NSIS 文件，签名与内置公钥匹配、受保护版本为 1.13.0；改动文件字节会拒绝验签。版本字段仅在验签成功后读取，与锁定 updater 的校验顺序一致。
- Windows Authenticode 尚未配置，不伪造证书或把 minisign 当作系统代码签名。
- 发布后复核：四个真实下载附件的 SHA-256/大小与 GitHub 记录一致，远端 NSIS 包通过同款 minisign 校验、版本绑定 1.13.0，篡改字节被拒绝；稳定端点只提供 Windows x64。详见 [发布验证](verification-v1.13.0.md)。

## 4. 客户端无法解决的问题

本项目采用 MIT 开源，无付费授权/订阅后端，不以混淆隐藏公开测速算法。运行时的 M-Lab 临时令牌仍会进入客户端内存，不能把客户端作为长期凭证保险箱。长期密钥不得随程序下发。

更新包验签不代表测量节点可信、所有依赖无漏洞或安装重启一定成功。

## 5. 修复顺序

- P0：排除真实长期凭证和私有数据。
- P1：核对真实发布包、签名、公钥及绑定版本；Windows Authenticode 尚未配置，保留该限制。
- P2：跟进原生控制流加固及长期资源/安装故障测试，不将工具启发式信号当作已证实漏洞。
