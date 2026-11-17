import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

export type Locale = "zh-CN" | "en-US";
export type LocaleSetting = "auto" | Locale;

const STORAGE_KEY = "netvision-pro.locale";

export function resolveLocale(setting: LocaleSetting, candidates: readonly string[] = []): Locale {
  if (setting === "zh-CN" || setting === "en-US") return setting;
  for (const candidate of candidates.length ? candidates : ["zh-CN"]) {
    const value = String(candidate).toLowerCase();
    if (value === "en" || value.startsWith("en-")) return "en-US";
    if (value === "zh" || value.startsWith("zh-")) return "zh-CN";
  }
  return "zh-CN";
}

function readSetting(): LocaleSetting {
  try {
    const value = window.localStorage.getItem(STORAGE_KEY);
    return value === "zh-CN" || value === "en-US" ? value : "auto";
  } catch {
    return "auto";
  }
}

const messages = {
  "zh-CN": {
    mainNav: "主导航",
    speedtest: "网络测速",
    quality: "网络质量",
    diagnostics: "网络诊断",
    history: "测速历史",
    servers: "测速节点",
    batchtest: "批量拨测",
    monitor: "网络监控",
    settings: "设置与关于",
    localEngine: "真实测速引擎 · 本地存储",
    windowTitle: "窗口标题栏",
    offline: "网络未连接",
    language: "界面语言",
    auto: "自动（跟随系统）",
    languageHint: "自动模式按浏览器或系统语言选择界面，无法识别时使用简体中文。",
    appContent: "主要内容",
    skip: "跳到主要内容",
    splash: "专业网络测速与诊断",
    retry: "重试本页",
    reload: "重新加载应用",
    renderError: "这个页面渲染出错",
    renderErrorHint: "应用的其他功能仍可正常使用。可以重试本页,或切换到其他页面后再回来。",
  },
  "en-US": {
    mainNav: "Main navigation",
    speedtest: "Speed test",
    quality: "Network quality",
    diagnostics: "Diagnostics",
    history: "Test history",
    servers: "Test servers",
    batchtest: "Batch tests",
    monitor: "Network monitor",
    settings: "Settings & about",
    localEngine: "Real test engine · Local storage",
    windowTitle: "Window title bar",
    offline: "Network disconnected",
    language: "Interface language",
    auto: "Automatic (system)",
    languageHint: "Automatic mode follows the browser or system language and falls back to English when unavailable.",
    appContent: "Main content",
    skip: "Skip to main content",
    splash: "Professional network speed and diagnostics",
    retry: "Retry this page",
    reload: "Reload application",
    renderError: "This page failed to render",
    renderErrorHint: "Other application features are still available. Retry this page or switch to another page and come back.",
  },
} as const;

type MessageKey = keyof typeof messages["zh-CN"];
interface I18nValue {
  setting: LocaleSetting;
  locale: Locale;
  setSetting: (setting: LocaleSetting) => void;
  t: (key: MessageKey) => string;
}
const I18nContext = createContext<I18nValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [setting, setSettingState] = useState<LocaleSetting>(() => readSetting());
  const locale = resolveLocale(setting, typeof navigator === "undefined" ? [] : navigator.languages || [navigator.language]);
  const setSetting = (next: LocaleSetting) => {
    setSettingState(next);
    try { window.localStorage.setItem(STORAGE_KEY, next); } catch { /* private mode */ }
  };
  useEffect(() => { document.documentElement.lang = locale; }, [locale]);
  const value = useMemo<I18nValue>(() => ({ setting, locale, setSetting, t: (key) => messages[locale][key] }), [setting, locale]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  const value = useContext(I18nContext);
  if (!value) throw new Error("useI18n must be used inside I18nProvider");
  return value;
}
