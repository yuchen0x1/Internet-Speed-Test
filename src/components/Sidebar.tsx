// 左侧导航
import { useApp, type PageId } from "../store/app";
import { Icon, type IconName } from "./Icon";

const NAV: { id: PageId; label: string; icon: IconName }[] = [
  { id: "speedtest", label: "网络测速", icon: "gauge" },
  { id: "quality", label: "网络质量", icon: "wave" },
  { id: "diagnostics", label: "网络诊断", icon: "shield" },
  { id: "history", label: "测速历史", icon: "history" },
  { id: "servers", label: "测速节点", icon: "server" },
  { id: "batchtest", label: "批量拨测", icon: "layers" },
  { id: "monitor", label: "网络监控", icon: "activity" },
  { id: "settings", label: "设置与关于", icon: "settings" },
];

export function Sidebar() {
  const { page, setPage, appInfo } = useApp();
  return (
    <nav className="sidebar">
      {NAV.map((n) => (
        <button
          key={n.id}
          className={`nav-item ${page === n.id ? "active" : ""}`}
          onClick={() => setPage(n.id)}
        >
          <span className="icon-wrap">
            <Icon name={n.icon} size={19} />
          </span>
          {n.label}
        </button>
      ))}
      <div className="sidebar-footer">
        NetVision Pro
        <br />
        v{appInfo?.version ?? "1.8.0"}
        <br />
        真实测速引擎 · 本地存储
      </div>
    </nav>
  );
}
