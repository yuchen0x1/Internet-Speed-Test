// 统一图标系统:1.6px 线性圆角风格,24px 网格
import React from "react";

export type IconName =
  | "gauge" | "wave" | "shield" | "history" | "server" | "activity" | "settings"
  | "play" | "stop" | "refresh" | "star" | "star-filled" | "download" | "upload"
  | "globe" | "wifi" | "ethernet" | "close" | "minus" | "square" | "copy"
  | "trash" | "export" | "check" | "alert" | "info" | "pause" | "chevron-right"
  | "chevron-down" | "clock" | "plus" | "target" | "cpu" | "layers" | "zap"
  | "search" | "eye" | "x-circle" | "trending";

const P: Record<IconName, React.ReactNode> = {
  gauge: <><path d="M12 21a9 9 0 1 1 9-9" /><path d="M12 12l5-3" /><circle cx="12" cy="12" r="1.2" fill="currentColor" stroke="none" /><path d="M21 12a9 9 0 0 1-.5 3" /></>,
  wave: <><path d="M3 12h3l2-7 3 14 3-10 2 5 2-2h3" /></>,
  shield: <><path d="M12 3l8 3v6c0 4.5-3.2 7.7-8 9-4.8-1.3-8-4.5-8-9V6l8-3z" /><path d="M9 12l2 2 4-4" /></>,
  history: <><path d="M3.5 12a8.5 8.5 0 1 0 2.5-6L3 9" /><path d="M3 4v5h5" /><path d="M12 8v4l3 2" /></>,
  server: <><rect x="3" y="4" width="18" height="7" rx="2" /><rect x="3" y="13" width="18" height="7" rx="2" /><path d="M7 7.5h.01M7 16.5h.01" /></>,
  activity: <><path d="M22 12h-4l-3 8-4-16-3 8H2" /></>,
  settings: <><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1 1.55V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1-1.55 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87 1.7 1.7 0 0 0-1.55-1H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.55-1 1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34h0a1.7 1.7 0 0 0 1-1.55V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1 1.55h0a1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87v0a1.7 1.7 0 0 0 1.55 1H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.55 1z" /></>,
  play: <><path d="M7 5l12 7-12 7V5z" /></>,
  stop: <><rect x="6" y="6" width="12" height="12" rx="2" /></>,
  refresh: <><path d="M21 12a9 9 0 1 1-2.6-6.3" /><path d="M21 3v6h-6" /></>,
  star: <><path d="M12 3l2.9 5.9 6.5.9-4.7 4.6 1.1 6.5L12 17.8 6.2 20.9l1.1-6.5L2.6 9.8l6.5-.9L12 3z" /></>,
  "star-filled": <><path d="M12 3l2.9 5.9 6.5.9-4.7 4.6 1.1 6.5L12 17.8 6.2 20.9l1.1-6.5L2.6 9.8l6.5-.9L12 3z" fill="currentColor" /></>,
  download: <><path d="M12 3v12" /><path d="M6 10l6 6 6-6" /><path d="M4 21h16" /></>,
  upload: <><path d="M12 17V5" /><path d="M6 10l6-6 6 6" /><path d="M4 21h16" /></>,
  globe: <><circle cx="12" cy="12" r="9" /><path d="M3 12h18" /><path d="M12 3a13.5 13.5 0 0 1 0 18 13.5 13.5 0 0 1 0-18z" /></>,
  wifi: <><path d="M2.5 9a15 15 0 0 1 19 0" /><path d="M5.5 12.5a10.5 10.5 0 0 1 13 0" /><path d="M8.8 16a6 6 0 0 1 6.4 0" /><circle cx="12" cy="19" r="1" fill="currentColor" stroke="none" /></>,
  ethernet: <><rect x="3" y="10" width="18" height="8" rx="2" /><path d="M7 10V6h10v4" /><path d="M8 18v-4M12 18v-4M16 18v-4" /></>,
  close: <><path d="M6 6l12 12M18 6L6 18" /></>,
  minus: <><path d="M5 12h14" /></>,
  square: <><rect x="6" y="6" width="12" height="12" rx="1.5" /></>,
  copy: <><rect x="9" y="9" width="12" height="12" rx="2" /><path d="M5 15V5a2 2 0 0 1 2-2h10" /></>,
  trash: <><path d="M4 7h16" /><path d="M9 7V4h6v3" /><path d="M6 7l1 13a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-13" /><path d="M10 11v6M14 11v6" /></>,
  export: <><path d="M12 15V3" /><path d="M7 8l5-5 5 5" /><path d="M4 15v4a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-4" /></>,
  check: <><path d="M4.5 12.5l5 5 10-11" /></>,
  alert: <><path d="M12 3l10 18H2L12 3z" /><path d="M12 10v5" /><circle cx="12" cy="18.2" r=".6" fill="currentColor" stroke="none" /></>,
  info: <><circle cx="12" cy="12" r="9" /><path d="M12 11v6" /><circle cx="12" cy="7.5" r=".6" fill="currentColor" stroke="none" /></>,
  pause: <><path d="M8 5v14M16 5v14" /></>,
  "chevron-right": <><path d="M9 5l7 7-7 7" /></>,
  "chevron-down": <><path d="M5 9l7 7 7-7" /></>,
  clock: <><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3.5 2" /></>,
  plus: <><path d="M12 5v14M5 12h14" /></>,
  target: <><circle cx="12" cy="12" r="9" /><circle cx="12" cy="12" r="4.5" /><circle cx="12" cy="12" r="1" fill="currentColor" stroke="none" /></>,
  cpu: <><rect x="6" y="6" width="12" height="12" rx="2" /><rect x="10" y="10" width="4" height="4" /><path d="M9 2v4M15 2v4M9 18v4M15 18v4M2 9h4M2 15h4M18 9h4M18 15h4" /></>,
  layers: <><path d="M12 3l9 5-9 5-9-5 9-5z" /><path d="M3 13l9 5 9-5" /></>,
  zap: <><path d="M13 2L4 14h6l-1 8 9-12h-6l1-8z" /></>,
  search: <><circle cx="11" cy="11" r="7" /><path d="M21 21l-4.5-4.5" /></>,
  eye: <><path d="M2 12s3.5-6.5 10-6.5S22 12 22 12s-3.5 6.5-10 6.5S2 12 2 12z" /><circle cx="12" cy="12" r="2.8" /></>,
  "x-circle": <><circle cx="12" cy="12" r="9" /><path d="M9 9l6 6M15 9l-6 6" /></>,
  trending: <><path d="M3 17l6-6 4 4 8-8" /><path d="M15 7h6v6" /></>,
};

export function Icon({ name, size = 20, className }: { name: IconName; size?: number; className?: string }) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      {P[name]}
    </svg>
  );
}
