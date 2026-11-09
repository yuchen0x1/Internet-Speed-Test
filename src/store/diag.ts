// 诊断中心跨页状态:检测结果提升到 store,切页重挂载后不丢失
import { create } from "zustand";
import type {
  ConnectivityEntry,
  DnsConsistencyReport,
  DnsTestReport,
  GatewayReport,
  HttpsResult,
  IpVersionCheck,
  LocalNetInfo,
  NatReport,
  PingReport,
  PublicInfo,
  RouteQualityReport,
  TcpResult,
  TraceHop,
  WifiDetails,
} from "../types";

interface DiagStore {
  local: LocalNetInfo | null;
  setLocal: (v: LocalNetInfo | null) => void;
  wifi: WifiDetails | null;
  setWifi: (v: WifiDetails | null) => void;
  pub: PublicInfo | null;
  setPub: (v: PublicInfo | null) => void;
  dns: DnsTestReport | null;
  setDns: (v: DnsTestReport | null) => void;
  dnsRun: boolean;
  setDnsRun: (v: boolean) => void;
  gw: GatewayReport | null;
  setGw: (v: GatewayReport | null) => void;
  gwRun: boolean;
  setGwRun: (v: boolean) => void;
  tcp: TcpResult[] | null;
  setTcp: (v: TcpResult[] | null) => void;
  tcpRun: boolean;
  setTcpRun: (v: boolean) => void;
  https: HttpsResult[] | null;
  setHttps: (v: HttpsResult[] | null) => void;
  httpsRun: boolean;
  setHttpsRun: (v: boolean) => void;
  v6: IpVersionCheck | null;
  setV6: (v: IpVersionCheck | null) => void;
  v6Run: boolean;
  setV6Run: (v: boolean) => void;
  routeHost: string;
  setRouteHost: (v: string) => void;
  route: RouteQualityReport | null;
  setRoute: (v: RouteQualityReport | null) => void;
  routeRun: boolean;
  setRouteRun: (v: boolean) => void;
  dnsDomain: string;
  setDnsDomain: (v: string) => void;
  dnsCons: DnsConsistencyReport | null;
  setDnsCons: (v: DnsConsistencyReport | null) => void;
  dnsConsRun: boolean;
  setDnsConsRun: (v: boolean) => void;
  nat: NatReport | null;
  setNat: (v: NatReport | null) => void;
  natRun: boolean;
  setNatRun: (v: boolean) => void;
  grid: ConnectivityEntry[] | null;
  setGrid: (v: ConnectivityEntry[] | null) => void;
  gridRun: boolean;
  setGridRun: (v: boolean) => void;
  traceHost: string;
  setTraceHost: (v: string) => void;
  trace: TraceHop[] | null;
  setTrace: (v: TraceHop[] | null) => void;
  traceRun: boolean;
  setTraceRun: (v: boolean) => void;
  pingHost: string;
  setPingHost: (v: string) => void;
  ping: PingReport | null;
  setPing: (v: PingReport | null) => void;
  pingRun: boolean;
  setPingRun: (v: boolean) => void;
}

export const useDiag = create<DiagStore>((set) => ({
  local: null,
  setLocal: (v) => set({ local: v }),
  wifi: null,
  setWifi: (v) => set({ wifi: v }),
  pub: null,
  setPub: (v) => set({ pub: v }),
  dns: null,
  setDns: (v) => set({ dns: v }),
  dnsRun: false,
  setDnsRun: (v) => set({ dnsRun: v }),
  gw: null,
  setGw: (v) => set({ gw: v }),
  gwRun: false,
  setGwRun: (v) => set({ gwRun: v }),
  tcp: null,
  setTcp: (v) => set({ tcp: v }),
  tcpRun: false,
  setTcpRun: (v) => set({ tcpRun: v }),
  https: null,
  setHttps: (v) => set({ https: v }),
  httpsRun: false,
  setHttpsRun: (v) => set({ httpsRun: v }),
  v6: null,
  setV6: (v) => set({ v6: v }),
  v6Run: false,
  setV6Run: (v) => set({ v6Run: v }),
  routeHost: "speed.cloudflare.com",
  setRouteHost: (v) => set({ routeHost: v }),
  route: null,
  setRoute: (v) => set({ route: v }),
  routeRun: false,
  setRouteRun: (v) => set({ routeRun: v }),
  dnsDomain: "www.baidu.com",
  setDnsDomain: (v) => set({ dnsDomain: v }),
  dnsCons: null,
  setDnsCons: (v) => set({ dnsCons: v }),
  dnsConsRun: false,
  setDnsConsRun: (v) => set({ dnsConsRun: v }),
  nat: null,
  setNat: (v) => set({ nat: v }),
  natRun: false,
  setNatRun: (v) => set({ natRun: v }),
  grid: null,
  setGrid: (v) => set({ grid: v }),
  gridRun: false,
  setGridRun: (v) => set({ gridRun: v }),
  traceHost: "speed.cloudflare.com",
  setTraceHost: (v) => set({ traceHost: v }),
  trace: null,
  setTrace: (v) => set({ trace: v }),
  traceRun: false,
  setTraceRun: (v) => set({ traceRun: v }),
  pingHost: "223.5.5.5",
  setPingHost: (v) => set({ pingHost: v }),
  ping: null,
  setPing: (v) => set({ ping: v }),
  pingRun: false,
  setPingRun: (v) => set({ pingRun: v }),
}));
