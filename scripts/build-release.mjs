// 本地签名发布构建:自动注入 TAURI_SIGNING_PRIVATE_KEY(_PASSWORD),
// 并通过 src-tauri/updater.conf.json 启用 updater 产物(安装包 + .sig)。
// 用法:npm run build:release [-- 透传给 tauri build 的额外参数]
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const keyPath = join(homedir(), ".tauri", "netvision-pro-updater.key");

let env = { ...process.env };
if (env.TAURI_SIGNING_PRIVATE_KEY) {
  console.log("使用环境变量中已设置的签名密钥");
} else {
  try {
    env.TAURI_SIGNING_PRIVATE_KEY = readFileSync(keyPath, "utf8");
    env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD = readFileSync(`${keyPath}.password`, "utf8");
    console.log(`已从 ${keyPath} 读取签名密钥`);
  } catch {
    console.error(`未找到签名密钥 ${keyPath}(及其 .password 文件)。`);
    console.error("调试构建请直接运行: npm run tauri -- build --bundles nsis");
    process.exit(1);
  }
}

const extra = process.argv.slice(2);
const res = spawnSync(
  "npx",
  ["tauri", "build", "--bundles", "nsis", "--config", "src-tauri/updater.conf.json", ...extra],
  { stdio: "inherit", shell: process.platform === "win32", env },
);
process.exit(res.status ?? 1);
