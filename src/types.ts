// 与 src-tauri/src 中 Rust 结构体一一对应的类型定义(serde 默认 camelCase 未启用,保持 snake_case)

export type Provider = "librespeed" | "cloudflare" | "ndt7" | "custom";

export interface ServerNodeInfo {
  id: string;
  name: string;
  provider: Provider;
  base_url: string;
  dl_url: string;
  ul_url: string;
  ping_url: string;
  getip_url: string;
  location: string | null;
  favorite: boolean;
  alive: boolean | null;
  latency_ms: number | null;
  allow_http: boolean;
}

export type Phase =
  | "idle"
  | "preparing"
  | "latency"
  | "download"
  | "upload"
  | "quality"
  | "finished"
  | "failed";

export interface QualityAssessment {
  score: number;
  rating: string;
  notes: string[];
}

export interface TestResult {
  id: string;
  started_at: string;
  finished_at: string;
  duration_ms: number;
  status: "completed" | "cancelled" | "failed";
  error: string | null;
  download_mbps: number;
  download_peak_mbps: number;
  upload_mbps: number | null;
  upload_peak_mbps: number | null;
  idle_ping_ms: number;
  ping_min_ms: number;
  ping_max_ms: number;
  jitter_ms: number;
  loaded_down_ms: number | null;
  loaded_up_ms: number | null;
  packet_loss_percent: number | null;
  http_failures: number;
  server: ServerNodeInfo | null;
  public_ip: string | null;
  isp: string | null;
  network_type: string;
  interface_name: string;
  down_samples: [number, number][];
  up_samples: [number, number][];
  latency_samples: [number, number][];
  quality: QualityAssessment | null;
  conditions: string[];
}

export type EngineEvent =
  | { type: "phase"; phase: string; message: string }
  | { type: "server"; node: ServerNodeInfo }
  | { type: "latency"; rtt_ms: number }
  | { type: "speed"; direction: "download" | "upload"; mbps: number; cumulative_mb: number }
  | { type: "loaded_latency"; direction: "download" | "upload"; rtt_ms: number }
  | { type: "progress"; percent: number }
  | { type: "finished"; result: TestResult }
  | { type: "failed"; error: string }
  | { type: "cancelled" };

export interface HistoryRow {
  id: number;
  created_at: string;
  download_mbps: number;
  upload_mbps: number | null;
  idle_ping_ms: number;
  jitter_ms: number;
  loaded_down_ms: number | null;
  loaded_up_ms: number | null;
  packet_loss_percent: number | null;
  server_name: string;
  server_provider: string;
  server_url: string;
  network_type: string;
  interface_name: string;
  public_ip: string | null;
  isp: string | null;
  status: string;
}

/** 历史导出结果:超出 5000 条上限时 exported < total */
export interface ExportOutcome {
  path: string;
  exported: number;
  total: number;
}

export interface IfaceTick {
  name: string;
  rx_bps: number;
  tx_bps: number;
  rx_total: number;
  tx_total: number;
  up: boolean;
}

export interface MonitorTick {
  ts_ms: number;
  rx_bps: number;
  tx_bps: number;
  rx_total: number;
  tx_total: number;
  ifaces: IfaceTick[];
}

export interface NetStatus {
  connected: boolean;
  kind: string;
  iface: string;
  link_mbps: number | null;
  ipv4: string | null;
  has_ipv6: boolean;
}

export interface IfaceInfo {
  name: string;
  friendly_name: string | null;
  description: string | null;
  kind: string;
  mac: string | null;
  ipv4: string[];
  ipv6: string[];
  gateway: string | null;
  dns_servers: string[];
  oper_up: boolean;
  link_mbps: number | null;
  mtu: number | null;
  is_default: boolean;
}

export interface LocalNetInfo {
  interfaces: IfaceInfo[];
  hostname: string;
  os: string;
}

export interface WifiDetails {
  ssid: string | null;
  bssid: string | null;
  signal_percent: number | null;
  radio_type: string | null;
  channel: string | null;
  receive_rate_mbps: number | null;
  transmit_rate_mbps: number | null;
}

export interface PublicInfo {
  allowed: boolean;
  ipv4: string | null;
  ipv6: string | null;
  isp: string | null;
  edge_colo: string | null;
  country: string | null;
}

export interface DnsDomainResult {
  domain: string;
  ok: boolean;
  ms: number | null;
  ips: string[];
  error: string | null;
}

export interface DnsServerProbe {
  server: string;
  ok: boolean;
  rtt_ms: number | null;
  answer_count: number;
  resolved_ip: string | null;
  error: string | null;
}

export interface DnsTestReport {
  system_servers: string[];
  domains: DnsDomainResult[];
  server_probes: DnsServerProbe[];
}

export interface GatewayReport {
  gateway: string | null;
  reachable: boolean;
  loss_percent: number | null;
  avg_ms: number | null;
  note: string | null;
}

export interface TcpResult {
  target: string;
  port: number;
  ok: boolean;
  ms: number | null;
  error: string | null;
}

export interface HttpsResult {
  url: string;
  ok: boolean;
  ms: number | null;
  status: number | null;
  error: string | null;
}

export interface TraceHop {
  hop: number;
  ip: string | null;
  times_ms: number[];
  timeout: boolean;
}

export interface IpVersionCheck {
  available: boolean;
  public_address: string | null;
  error: string | null;
}

export interface PingReport {
  host: string;
  sent: number;
  loss_percent: number | null;
  rtts_ms: number[];
  avg_ms: number | null;
}

export interface AppSettings {
  theme: "dark" | "light" | "system";
  animations: boolean;
  anim_intensity: number;
  refresh_interval_ms: number;
  node_mode: "auto" | "manual";
  default_node_url: string;
  test_duration_secs: number;
  precision_mode: boolean;
  down_concurrency: number;
  up_concurrency: number;
  autosave_history: boolean;
  monitor_enabled: boolean;
  show_traffic: boolean;
  autostart: boolean;
  auto_test_enabled: boolean;
  auto_test_interval_minutes: number;
  notify_events: boolean;
  alert_enabled: boolean;
  alert_threshold_ms: number;
  alert_target: string;
  retention_days: number;
  prometheus_enabled: boolean;
  prometheus_port: number;
  show_public_ip: boolean;
  save_diag_history: boolean;
  close_to_tray: boolean;
  custom_nodes: ServerNodeInfo[];
}

export interface AppInfo {
  name: string;
  version: string;
  os: string;
  data_dir: string;
}

export type ProbeResult = {
  id: string;
  alive: boolean;
  latencyMs: number | null;
};

export interface ConnectivityEntry {
  name: string;
  host: string;
  ok: boolean;
  ms: number | null;
  error: string | null;
}

export interface BatchTargetResult {
  target: string;
  sent: number;
  recv: number;
  loss_percent: number;
  min_ms: number | null;
  max_ms: number | null;
  avg_ms: number | null;
  latest_ms: number | null;
  samples: (number | null)[];
}

export interface DnsDistEntry {
  server: string;
  source: string;
  ip: string;
  rtt_ms: number | null;
}

export interface DnsDistributionReport {
  domain: string;
  entries: DnsDistEntry[];
  distribution: [string, number][];
  resolvers: number;
}

export interface HopQuality {
  hop: number;
  ip: string | null;
  loss_percent: number | null;
  avg_ms: number | null;
  min_ms: number | null;
  max_ms: number | null;
  jitter_ms: number | null;
  samples: number;
  timeout: boolean;
}

export interface RouteQualityReport {
  host: string;
  hops: HopQuality[];
}

export interface DnsConsistencyEntry {
  server: string;
  source: "系统" | "公共";
  ok: boolean;
  rtt_ms: number | null;
  ips: string[];
  error: string | null;
}

export interface DnsConsistencyReport {
  domain: string;
  entries: DnsConsistencyEntry[];
  verdict: string;
  note: string | null;
}

export interface NatServerEntry {
  server: string;
  ok: boolean;
  mapped_ip: string | null;
  mapped_port: number | null;
  error: string | null;
}

export interface NatReport {
  nat_type: string;
  local_ip: string | null;
  servers: NatServerEntry[];
  note: string | null;
}
