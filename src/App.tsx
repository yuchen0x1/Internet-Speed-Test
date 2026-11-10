// 应用外壳:启动动画 / 主题 / 事件绑定 / 页面路由
import { lazy, Suspense, useEffect, useState } from "react";
import { useApp } from "./store/app";
import { useTest } from "./store/test";
import { TitleBar } from "./components/TitleBar";
import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { Toasts } from "./components/ui";
import { Icon } from "./components/Icon";
import { ErrorBoundary, installGlobalErrorHandlers } from "./components/ErrorBoundary";
import { SpeedTestPage } from "./pages/SpeedTestPage";
// 首屏是测速页,保持同步加载;其余页面按需加载以降低首屏包体积
const QualityPage = lazy(() => import("./pages/QualityPage").then((m) => ({ default: m.QualityPage })));
const DiagnosticsPage = lazy(() => import("./pages/DiagnosticsPage").then((m) => ({ default: m.DiagnosticsPage })));
const HistoryPage = lazy(() => import("./pages/HistoryPage").then((m) => ({ default: m.HistoryPage })));
const ServersPage = lazy(() => import("./pages/ServersPage").then((m) => ({ default: m.ServersPage })));
const BatchTestPage = lazy(() => import("./pages/BatchTestPage").then((m) => ({ default: m.BatchTestPage })));
const MonitorPage = lazy(() => import("./pages/MonitorPage").then((m) => ({ default: m.MonitorPage })));
const SettingsPage = lazy(() => import("./pages/SettingsPage").then((m) => ({ default: m.SettingsPage })));

export default function App() {
  // 逐字段订阅:toast/网络状态等变化不再触发 App 及整棵页面树重渲染
  const booted = useApp((s) => s.booted);
  const page = useApp((s) => s.page);
  const boot = useApp((s) => s.boot);
  const settings = useApp((s) => s.settings);
  const bindTest = useTest((s) => s.bind);
  const syncRunning = useTest((s) => s.syncRunning);
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

  // 全局错误兜底:未捕获异常/未处理 Promise 不静默丢失
  useEffect(() => {
    return installGlobalErrorHandlers((kind, text) => useApp.getState().toast(kind, text));
  }, []);

  // 与后端引擎对账:托盘启动或窗口重载后,前端可能错过事件而状态停在 idle。
  // 启动完成时对一次,窗口重新获得焦点时再对一次。
  useEffect(() => {
    if (!booted) return;
    syncRunning();
    const onFocus = () => syncRunning();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [booted, syncRunning]);

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
              {/* 单页渲染失败只影响该页,不卸载整个应用;切页后自动复位 */}
              <ErrorBoundary resetKey={page}>
                <Suspense fallback={<div className="empty">页面加载中…</div>}>
                  {page === "speedtest" && <SpeedTestPage />}
                  {page === "quality" && <QualityPage />}
                  {page === "diagnostics" && <DiagnosticsPage />}
                  {page === "history" && <HistoryPage />}
                  {page === "servers" && <ServersPage />}
                  {page === "batchtest" && <BatchTestPage />}
                  {page === "monitor" && <MonitorPage />}
                  {page === "settings" && <SettingsPage />}
                </Suspense>
              </ErrorBoundary>
            </div>
          </main>
        </div>
        <StatusBar />
      </div>
      <Toasts />
    </>
  );
}
