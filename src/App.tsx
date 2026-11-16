// 应用外壳:启动动画 / 主题 / 事件绑定 / 页面路由
import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { api } from "./api";
import { useApp, type PageId } from "./store/app";
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
  const mainRef = useRef<HTMLElement>(null);

  useEffect(() => {
    boot();
    bindTest().catch((e) => useApp.getState().toast("error", `测速事件连接失败，重新聚焦窗口可重试：${String(e)}`));
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

  // 键盘用户不必先定位到测速页按钮即可控制当前测速。
  // Ctrl/Cmd+Enter 开始或停止，Escape 取消；输入控件内保留原生编辑语义。
  useEffect(() => {
    if (!booted) return;
    const isEditable = (target: EventTarget | null) => {
      const el = target as HTMLElement | null;
      return !!el && (el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName));
    };
    const onKey = (e: KeyboardEvent) => {
      if (isEditable(e.target)) return;
      const test = useTest.getState();
      if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key === "Enter") {
        e.preventDefault();
        const action = test.running ? api.speedtestStop() : api.speedtestStart();
        action.catch((error) => useApp.getState().toast("error", `测速操作失败：${String(error)}`));
        return;
      }
      if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && /^[1-8]$/.test(e.key)) {
        const pages: PageId[] = [
          "speedtest", "quality", "diagnostics", "history",
          "servers", "batchtest", "monitor", "settings",
        ];
        const next = pages[Number(e.key) - 1];
        if (next) {
          e.preventDefault();
          useApp.getState().setPage(next);
          return;
        }
      }
      if (e.key === "Escape" && test.running) {
        e.preventDefault();
        api.speedtestStop().catch((error) => useApp.getState().toast("error", `取消测速失败：${String(error)}`));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [booted]);

  // 页面切换后把焦点放回内容区，避免键盘用户继续操作已卸载页面的控件。
  useEffect(() => {
    if (!booted) return;
    const title = {
      speedtest: "网络测速",
      quality: "网络质量",
      diagnostics: "网络诊断",
      history: "测速历史",
      servers: "测速节点",
      batchtest: "批量拨测",
      monitor: "网络监控",
      settings: "设置与关于",
    }[page];
    document.title = title ? `${title} · NetVision Pro` : "NetVision Pro";
    const focus = window.setTimeout(() => mainRef.current?.focus(), 0);
    return () => window.clearTimeout(focus);
  }, [booted, page]);

  // 与后端引擎对账:托盘启动或窗口重载后,前端可能错过事件而状态停在 idle。
  // 启动完成时对一次,窗口重新获得焦点时再对一次。
  useEffect(() => {
    if (!booted) return;
    const onFocus = async () => {
      try {
        await bindTest();
        await syncRunning();
      } catch (e) {
        useApp.getState().toast("error", `测速事件连接失败：${String(e)}`);
      }
    };
    onFocus();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [booted, bindTest, syncRunning]);

  // 启动后按用户设置检查一次稳定版更新。时间戳写入本地 WebView 存储，
  // 避免 StrictMode 重复挂载和频繁启动造成重复请求；检查失败不阻塞应用启动。
  useEffect(() => {
    if (!booted || !settings?.auto_update_enabled) return;
    const intervalMs = Math.max(6, settings.update_check_interval_hours || 24) * 60 * 60 * 1000;
    const key = "netvision-pro.update.last-check-at";
    let last = 0;
    try {
      last = Number(window.localStorage.getItem(key) ?? 0);
    } catch {
      // WebView storage may be unavailable in a restricted profile; update checks remain optional.
    }
    if (Number.isFinite(last) && Date.now() - last < intervalMs) return;
    try {
      window.localStorage.setItem(key, String(Date.now()));
    } catch {
      // Continue with a one-shot check without persistence.
    }
    api.updaterCheck().then((update) => {
      if (update) {
        useApp.getState().toast("info", `发现稳定版更新 v${update.version}，请前往设置页查看`);
      }
    }).catch(() => {
      // 网络不可用时保持当前版本可用，手动检查仍可在设置页重试。
      try { window.localStorage.removeItem(key); } catch { /* ignore storage errors */ }
    });
  }, [booted, settings?.auto_update_enabled, settings?.update_check_interval_hours]);

  useEffect(() => {
    if (!booted) return;
    const t = window.setTimeout(() => setSplash(false), settings?.animations ? 900 : 150);
    return () => window.clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [booted]);

  return (
    <>
      <a className="skip-link" href="#main-content">跳到主要内容</a>
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
          <main
            className="main"
            id="main-content"
            ref={mainRef}
            key={page}
            tabIndex={-1}
            aria-label="主要内容"
          >
            <div className="page-enter">
              {/* 单页渲染失败只影响该页,不卸载整个应用;切页后自动复位 */}
              <ErrorBoundary resetKey={page}>
                <Suspense fallback={<div className="empty" role="status" aria-live="polite">页面加载中…</div>}>
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
