// 应用外壳:启动动画 / 主题 / 事件绑定 / 页面路由
import { useEffect, useState } from "react";
import { useApp } from "./store/app";
import { useTest } from "./store/test";
import { TitleBar } from "./components/TitleBar";
import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { Toasts } from "./components/ui";
import { Icon } from "./components/Icon";
import { SpeedTestPage } from "./pages/SpeedTestPage";
import { QualityPage } from "./pages/QualityPage";
import { DiagnosticsPage } from "./pages/DiagnosticsPage";
import { HistoryPage } from "./pages/HistoryPage";
import { ServersPage } from "./pages/ServersPage";
import { BatchTestPage } from "./pages/BatchTestPage";
import { MonitorPage } from "./pages/MonitorPage";
import { SettingsPage } from "./pages/SettingsPage";

export default function App() {
  // 逐字段订阅:toast/网络状态等变化不再触发 App 及整棵页面树重渲染
  const booted = useApp((s) => s.booted);
  const page = useApp((s) => s.page);
  const boot = useApp((s) => s.boot);
  const settings = useApp((s) => s.settings);
  const bindTest = useTest((s) => s.bind);
  const [splash, setSplash] = useState(true);

  useEffect(() => {
    boot();
    bindTest();
    // 跟随系统主题监听
    const mq = window.matchMedia("(prefers-color-scheme: light)");
    const onChange = () => useApp.getState().applyTheme();
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [boot, bindTest]);

  useEffect(() => {
    if (!booted) return;
    const t = window.setTimeout(() => setSplash(false), settings?.animations ? 900 : 150);
    return () => window.clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [booted]);

  return (
    <>
      {splash && (
        <div className={`splash ${!booted ? "waiting" : "leaving"}`}>
          <div className="splash-logo">
            <Icon name="gauge" size={44} />
          </div>
          <div className="splash-name">NetVision Pro</div>
          <div className="splash-sub">专业网络测速与诊断</div>
          <div className="splash-bar"><div /></div>
        </div>
      )}
      <div className="app">
        <TitleBar />
        <div className="app-body">
          <Sidebar />
          <main className="main" key={page}>
            <div className="page-enter">
              {page === "speedtest" && <SpeedTestPage />}
              {page === "quality" && <QualityPage />}
              {page === "diagnostics" && <DiagnosticsPage />}
              {page === "history" && <HistoryPage />}
              {page === "servers" && <ServersPage />}
              {page === "batchtest" && <BatchTestPage />}
              {page === "monitor" && <MonitorPage />}
              {page === "settings" && <SettingsPage />}
            </div>
          </main>
        </div>
        <StatusBar />
      </div>
      <Toasts />
    </>
  );
}
