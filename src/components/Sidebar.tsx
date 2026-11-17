// 左侧导航:按钮天然可聚焦/可回车触发;补 aria-current 与 Ctrl+1..8 快捷切换
import { useApp, type PageId } from "../store/app";
import { Icon, type IconName } from "./Icon";
import { useI18n } from "../i18n";

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
  const { t } = useI18n();
  const page = useApp((s) => s.page);
  const setPage = useApp((s) => s.setPage);
  const appInfo = useApp((s) => s.appInfo);

  return (
    <nav className="sidebar" aria-label={t("mainNav")}>
      {NAV.map((n, i) => {
        const active = page === n.id;
        return (
          <button
            key={n.id}
            className={`nav-item ${active ? "active" : ""}`}
            onClick={() => setPage(n.id)}
            aria-current={active ? "page" : undefined}
            aria-keyshortcuts={`Ctrl+${i + 1}`}
            title={`${t(n.id)} (Ctrl+${i + 1})`}
          >
            <span className="icon-wrap" aria-hidden="true">
              <Icon name={n.icon} size={19} />
            </span>
            {t(n.id)}
          </button>
        );
      })}
      <div className="sidebar-footer">
        NetVision Pro
        <br />
        v{appInfo?.version ?? "—"}
        <br />
        {t("localEngine")}
      </div>
    </nav>
  );
}
