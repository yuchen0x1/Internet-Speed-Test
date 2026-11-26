// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DiagnosticsPage } from "./DiagnosticsPage";
import { api } from "../api";
import { useApp } from "../store/app";
import { useDiag } from "../store/diag";
import type { DnsTestReport, GatewayReport, LocalNetInfo } from "../types";

vi.mock("../api", () => ({
  api: {
    diagLocal: vi.fn(), diagWifi: vi.fn(), diagPublic: vi.fn(),
    diagDns: vi.fn(), diagGateway: vi.fn(), diagTcp: vi.fn(),
    diagHttps: vi.fn(), diagIpv6: vi.fn(), diagConnectivityGrid: vi.fn(),
  },
  errMsg: (error: unknown) => String(error),
}));

let root: Root;
let host: HTMLDivElement;
const toast = vi.fn();
const local: LocalNetInfo = { hostname: "fixture", os: "fixture", interfaces: [] } as LocalNetInfo;
const dns: DnsTestReport = { domains: [], server_probes: [] } as unknown as DnsTestReport;
const button = (text: string) => [...host.querySelectorAll("button")].find((item) => item.textContent?.includes(text))!;

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.resetAllMocks();
  useDiag.setState(useDiag.getInitialState());
  useApp.setState({ toast, settings: null });
  vi.mocked(api.diagLocal).mockResolvedValue(local);
  vi.mocked(api.diagWifi).mockResolvedValue(null);
  vi.mocked(api.diagPublic).mockResolvedValue({} as Awaited<ReturnType<typeof api.diagPublic>>);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

describe("diagnostic page requests", () => {
  it("handles initial information failure without an unhandled rejection", async () => {
    vi.mocked(api.diagLocal).mockRejectedValueOnce(new Error("offline"));
    await act(async () => root.render(<DiagnosticsPage />));
    expect(toast).toHaveBeenCalledWith("error", expect.stringContaining("本地网络信息获取失败"));
  });

  it("drops late bulk results and does not start queued checks after leaving the page", async () => {
    let finishBase!: (value: LocalNetInfo) => void;
    let finishDns!: (value: DnsTestReport) => void;
    let finishGateway!: (value: GatewayReport) => void;
    await act(async () => root.render(<DiagnosticsPage />));
    vi.mocked(api.diagLocal).mockImplementationOnce(() => new Promise((resolve) => { finishBase = resolve; }));
    vi.mocked(api.diagDns).mockImplementationOnce(() => new Promise((resolve) => { finishDns = resolve; }));
    vi.mocked(api.diagGateway).mockImplementationOnce(() => new Promise((resolve) => { finishGateway = resolve; }));
    await act(async () => {
      button("运行基础检查").click();
      button("运行基础检查").click();
    });
    expect(api.diagDns).toHaveBeenCalledOnce();
    expect(api.diagGateway).toHaveBeenCalledOnce();
    await act(async () => root.render(null));
    await act(async () => root.render(<DiagnosticsPage />));

    let finishNewDns!: (value: DnsTestReport) => void;
    vi.mocked(api.diagDns).mockImplementationOnce(() => new Promise((resolve) => { finishNewDns = resolve; }));
    await act(async () => button("开始检测").click());
    expect(useDiag.getState().dnsRun).toBe(true);
    await act(async () => {
      finishBase({ ...local, hostname: "stale-host" });
      finishDns(dns);
      finishGateway({} as GatewayReport);
    });
    expect(useDiag.getState().local?.hostname).toBe("fixture");
    expect(useDiag.getState().dns).toBeNull();
    expect(useDiag.getState().dnsRun).toBe(true);
    expect(api.diagTcp).not.toHaveBeenCalled();
    expect(api.diagHttps).not.toHaveBeenCalled();
    expect(api.diagIpv6).not.toHaveBeenCalled();
    expect(api.diagConnectivityGrid).not.toHaveBeenCalled();
    expect(toast).not.toHaveBeenCalledWith("success", expect.stringContaining("基础诊断完成"));
    await act(async () => finishNewDns(dns));
    expect(useDiag.getState().dnsRun).toBe(false);
    expect(useDiag.getState().dns).toEqual(dns);
  });
});
