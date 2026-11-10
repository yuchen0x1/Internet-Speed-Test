// Tauri IPC 封装:统一错误处理;事件监听集中管理

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppInfo,
  AppSettings,
  BatchTargetResult,
  DnsDistributionReport,
  ConnectivityEntry,
  DiagRunRow,
  DnsConsistencyReport,
  DnsTestReport,
  ExportOutcome,
  GatewayReport,
  HistoryRow,
  HttpsResult,
  IpVersionCheck,
  LocalNetInfo,
  MonitorTick,
  NatReport,
  NetStatus,
  PingReport,
  PublicInfo,
  PrometheusStatus,
  RouteQualityReport,
  ServerNodeInfo,
  TcpResult,
  TestResult,
  TraceHop,
  WifiDetails,
} from "./types";

/** 携带错误码的调用错误(诊断命令的 CommandError 经 IPC 透传 code) */
export class AppInvokeError extends Error {
  readonly code: string;
  constructor(message: string, code = "failed") {
    super(message);
    this.name = "AppInvokeError";
    this.code = code;
  }
}

/** 展示用错误文案:AppInvokeError 取 message,其余走 String() */
export const errMsg = (e: unknown): string =>
  e instanceof AppInvokeError ? e.message : String(e);

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await tauriInvoke<T>(cmd, args);
  } catch (e) {
    // 诊断命令返回 { code, message } 结构化错误,保留 code 供分支判断;
    // 字符串/普通 Error 统一包装为 failed 码,全部调用方拿到同一错误类型
    if (e && typeof e === "object" && typeof (e as { message?: unknown }).message === "string") {
      const ce = e as { code?: unknown; message: string };
      throw new AppInvokeError(ce.message, typeof ce.code === "string" ? ce.code : "failed");
    }
    throw new AppInvokeError(typeof e === "string" ? e : JSON.stringify(e));
  }
}

export const api = {
  appInfo: () => invoke<AppInfo>("get_app_info"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  setSettings: (settings: AppSettings) => invoke<void>("set_settings", { settings }),
  netStatus: () => invoke<NetStatus>("get_network_status"),

  serversList: () => invoke<ServerNodeInfo[]>("servers_list"),
  serversRefresh: () => invoke<ServerNodeInfo[]>("servers_refresh"),
  serversSetFavorite: (url: string, name: string, favorite: boolean) =>
    invoke<void>("servers_set_favorite", { url, name, favorite }),
  serversSetManual: (url: string) => invoke<void>("servers_set_manual", { url }),
  serversSetAuto: () => invoke<void>("servers_set_auto"),
  serversAddCustom: (node: Partial<ServerNodeInfo>) =>
    invoke<ServerNodeInfo[]>("servers_add_custom", { node }),
  serversRemoveCustom: (url: string) => invoke<ServerNodeInfo[]>("servers_remove_custom", { url }),

  speedtestStart: () => invoke<void>("speedtest_start"),
  speedtestStop: () => invoke<void>("speedtest_stop"),
  speedtestRunning: () => invoke<boolean>("speedtest_running"),
  speedtestPhase: () => invoke<string>("speedtest_phase"),
  openSpeedtestService: (service: string) => invoke<void>("open_speedtest_service", { service }),

  historyList: (opts?: { from?: string; to?: string; desc?: boolean; limit?: number; cursorId?: number }) =>
    invoke<HistoryRow[]>("history_list", {
      from: opts?.from ?? null,
      to: opts?.to ?? null,
      desc: opts?.desc ?? true,
      limit: opts?.limit ?? 500,
      cursorId: opts?.cursorId ?? null,
    }),
  historyDetail: (id: number) => invoke<TestResult | null>("history_detail", { id }),
  historyCount: (opts?: { from?: string; to?: string }) =>
    invoke<number>("history_count", {
      from: opts?.from ?? null,
      to: opts?.to ?? null,
    }),
  historyDelete: (id: number) => invoke<void>("history_delete", { id }),
  historyClear: () => invoke<void>("history_clear"),
  historyExport: (format: "csv" | "json") =>
    invoke<ExportOutcome>("history_export", { format }),
  historyExportReport: (id: number) => invoke<string>("history_export_report", { id }),

  monitorSetEnabled: (enabled: boolean) => invoke<void>("monitor_set_enabled", { enabled }),
  monitorPaused: () => invoke<boolean>("monitor_paused"),
  monitorSetPaused: (paused: boolean) => invoke<void>("monitor_set_paused", { paused }),
  monitorHistory: (windowSecs: number) =>
    invoke<MonitorTick[]>("monitor_history", { windowSecs }),
  monitorLatest: () => invoke<MonitorTick | null>("monitor_latest"),
  prometheusStatus: () => invoke<PrometheusStatus>("prometheus_status"),

  diagLocal: () => invoke<LocalNetInfo>("diag_local_network"),
  diagHistoryList: (opts?: { kind?: string; limit?: number }) =>
    invoke<DiagRunRow[]>("diag_history_list", {
      kind: opts?.kind ?? null,
      limit: opts?.limit ?? 200,
    }),
  diagHistoryDetail: (id: number) => invoke<string | null>("diag_history_detail", { id }),
  diagHistoryClear: () => invoke<number>("diag_history_clear"),
  diagWifi: () => invoke<WifiDetails | null>("diag_wifi_details"),
  diagPublic: () => invoke<PublicInfo>("diag_public_info"),
  diagDns: () => invoke<DnsTestReport>("diag_dns_test"),
  diagGateway: () => invoke<GatewayReport>("diag_gateway_test"),
  diagTcp: () => invoke<TcpResult[]>("diag_tcp_test", { targets: null }),
  diagHttps: () => invoke<HttpsResult[]>("diag_https_test"),
  diagTraceroute: (host: string, maxHops?: number) =>
    invoke<TraceHop[]>("diag_traceroute", { host, maxHops: maxHops ?? 15 }),
  diagPing: (host: string, count?: number) =>
    invoke<PingReport>("diag_ping_test", { host, count: count ?? 8 }),
  diagIpv6: () => invoke<IpVersionCheck>("diag_ipv6_check"),
  diagRouteQuality: (host: string, maxHops?: number, probes?: number) =>
    invoke<RouteQualityReport>("diag_route_quality", {
      host,
      maxHops: maxHops ?? 12,
      probes: probes ?? 6,
    }),
  diagDnsConsistency: (domain: string) =>
    invoke<DnsConsistencyReport>("diag_dns_consistency", { domain }),
  diagNatType: () => invoke<NatReport>("diag_nat_type"),
  diagConnectivityGrid: () => invoke<ConnectivityEntry[]>("diag_connectivity_grid"),
  pingmonStart: (host: string) => invoke<void>("pingmon_start", { host }),
  pingmonStop: () => invoke<void>("pingmon_stop"),
  pingmonHost: () => invoke<string | null>("pingmon_host"),
  copyResultText: (result: TestResult) => invoke<string>("copy_result_text", { result }),
  batchtestStart: (targets: string[], count: number) =>
    invoke<number>("batchtest_start", { targets, count }),
  batchtestStop: () => invoke<void>("batchtest_stop"),
  batchtestRunning: () => invoke<boolean>("batchtest_running"),
  dnsDistribution: (domain: string) =>
    invoke<DnsDistributionReport>("dns_distribution", { domain }),
};

/** 测速引擎事件 */
export function onEngineEvent(cb: (ev: import("./types").EngineEvent) => void): Promise<UnlistenFn> {
  return listen<import("./types").EngineEvent>("speedtest://event", (e) => cb(e.payload));
}

/** 节点探测进度事件 */
export function onServersProbed(
  cb: (probes: { id: string; alive: boolean; latencyMs: number | null }[]) => void,
): Promise<UnlistenFn> {
  return listen<{ id: string; alive: boolean; latencyMs: number | null }[]>(
    "servers://probed",
    (e) => cb(e.payload),
  );
}

/** 网络监控每秒事件 */
export function onMonitorTick(cb: (tick: MonitorTick) => void): Promise<UnlistenFn> {
  return listen<MonitorTick>("monitor://tick", (e) => cb(e.payload));
}

/** 网络状态变化事件 */
export function onNetStatus(cb: (s: NetStatus) => void): Promise<UnlistenFn> {
  return listen<NetStatus>("net://status", (e) => cb(e.payload));
}

/** 路由追踪逐跳事件 */
export function onTraceHop(cb: (hop: TraceHop) => void): Promise<UnlistenFn> {
  return listen<TraceHop>("diag://trace", (e) => cb(e.payload));
}

/** 批量拨测事件 */
export function onBatchTick(cb: (t: { target: string; rtt_ms: number | null }) => void): Promise<UnlistenFn> {
  return listen<{ target: string; rtt_ms: number | null }>("batchtest://tick", (e) => cb(e.payload));
}

export function onBatchTargetDone(cb: (r: BatchTargetResult) => void): Promise<UnlistenFn> {
  return listen<BatchTargetResult>("batchtest://target_done", (e) => cb(e.payload));
}

export function onBatchAllDone(cb: (n: number) => void): Promise<UnlistenFn> {
  return listen<number>("batchtest://all_done", (e) => cb(e.payload));
}

/** 连续 Ping 监测事件 */
export function onPingMonTick(cb: (t: { ts_ms: number; rtt_ms: number | null }) => void): Promise<UnlistenFn> {
  return listen<{ ts_ms: number; rtt_ms: number | null }>("pingmon://tick", (e) => cb(e.payload));
}
