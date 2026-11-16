// IPC 错误规范化契约:诊断命令的 { code, message } 经 invoke 包装后保留错误码
import { describe, expect, it, vi } from "vitest";
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { api, AppInvokeError, errMsg, invoke } from "./api";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const mockInvoke = vi.mocked(tauriInvoke);

it("passes date and order through history export and retains default whole-history calls", async () => {
  mockInvoke.mockResolvedValueOnce({ path: "fixture.csv", exported: 2, total: 2 });
  await api.historyExport("csv", { from: "2026-11-07", to: "2026-11-07", desc: false });
  expect(mockInvoke).toHaveBeenLastCalledWith("history_export", {
    format: "csv", from: "2026-11-07", to: "2026-11-07", query: null, status: null, desc: false,
  });
  mockInvoke.mockResolvedValueOnce({ path: "fixture.json", exported: 2, total: 2 });
  await api.historyExport("json");
  expect(mockInvoke).toHaveBeenLastCalledWith("history_export", { format: "json", from: null, to: null, query: null, status: null, desc: true });
});

describe("invoke 错误规范化", () => {
  it("结构化错误保留 code 与 message", async () => {
    mockInvoke.mockRejectedValueOnce({ code: "invalid_host", message: "无效的域名" });
    await expect(invoke("diag_dns_consistency", { domain: "x" })).rejects.toMatchObject({
      code: "invalid_host",
      message: "无效的域名",
    });
  });

  it("字符串错误包装为 failed 码", async () => {
    mockInvoke.mockRejectedValueOnce("节点不可用");
    const e = (await invoke("history_list").catch((err) => err)) as AppInvokeError;
    expect(e).toBeInstanceOf(AppInvokeError);
    expect(e.code).toBe("failed");
    expect(e.message).toBe("节点不可用");
  });

  it("对象缺 code 时兜底 failed", async () => {
    mockInvoke.mockRejectedValueOnce({ message: "只有消息" });
    const e = (await invoke("diag_nat_type").catch((err) => err)) as AppInvokeError;
    expect(e.code).toBe("failed");
    expect(e.message).toBe("只有消息");
  });
});

describe("historyList pagination", () => {
  it("passes filter, order, limit, and cursor to the history IPC", async () => {
    mockInvoke.mockResolvedValueOnce([]);

    await api.historyList({ from: "2026-11-01", to: "2026-11-10", desc: false, limit: 100, cursorId: 200 });

    expect(mockInvoke).toHaveBeenCalledWith("history_list", {
      from: "2026-11-01",
      to: "2026-11-10",
      query: null,
      status: null,
      desc: false,
      limit: 100,
      cursorId: 200,
    });
  });

  it("defaults omitted cursor to null", async () => {
    mockInvoke.mockResolvedValueOnce([]);

    await api.historyList();

    expect(mockInvoke).toHaveBeenCalledWith("history_list", {
      from: null,
      to: null,
      query: null,
      status: null,
      desc: true,
      limit: 500,
      cursorId: null,
    });
  });
});

describe("设置备份 IPC", () => {
  it("exports and imports through the dedicated commands", async () => {
    mockInvoke.mockResolvedValueOnce("C:/backup/netvision-settings.json");
    await expect(api.settingsExport()).resolves.toBe("C:/backup/netvision-settings.json");
    expect(mockInvoke).toHaveBeenLastCalledWith("settings_export", undefined);

    const imported = { theme: "dark" } as never;
    mockInvoke.mockResolvedValueOnce(imported);
    await expect(api.settingsImport()).resolves.toBe(imported);
    expect(mockInvoke).toHaveBeenLastCalledWith("settings_import", undefined);
  });
});

it("passes monitor export window and format to the native command", async () => {
  mockInvoke.mockResolvedValueOnce("fixture.csv");

  await api.monitorExport(300, "csv");

  expect(mockInvoke).toHaveBeenCalledWith("monitor_export", {
    windowSecs: 300,
    format: "csv",
  });
});

it("passes the diagnostics report format and snapshot to the native command", async () => {
  mockInvoke.mockResolvedValueOnce("fixture.html");
  const report = { schema: "netvision-pro-diagnostics", schema_version: 1, checks: { ping: null } };
  await api.diagExportReport("html", report);
  expect(mockInvoke).toHaveBeenCalledWith("diag_export_report", { format: "html", report });
});

it("passes monitor aggregation window and supported bucket to the native command", async () => {
  mockInvoke.mockResolvedValueOnce([]);

  await api.monitorAggregate(3600, 300);

  expect(mockInvoke).toHaveBeenCalledWith("monitor_aggregate", {
    windowSecs: 3600,
    bucketSecs: 300,
  });
});

describe("errMsg 展示文案", () => {
  it("AppInvokeError 直接取 message(不带 Error: 前缀)", () => {
    expect(errMsg(new AppInvokeError("无效的域名", "invalid_host"))).toBe("无效的域名");
  });
  it("其余走 String()", () => {
    expect(errMsg("plain")).toBe("plain");
    expect(errMsg(new Error("boom"))).toBe("Error: boom");
  });
});
