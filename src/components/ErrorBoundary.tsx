// 页面级错误边界 + 全局错误兜底。
//
// 桌面应用没有"刷新页面"直觉:任何一页渲染抛错都必须留下可恢复的 UI,
// 而不是整棵树卸载成白屏。这里同时接管未捕获异常与未处理 Promise 拒绝,
// 统一走 toast + 日志,不静默吞掉。
import React from "react";
import { Icon } from "./Icon";

interface Props {
  children: React.ReactNode;
  /** 切换页面时作为 key 传入,便于用户换页后自动复位错误态 */
  resetKey?: string;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends React.Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    // 保留堆栈到控制台日志;tauri-plugin-log 会落盘,便于事后排查
    console.error("[ErrorBoundary] 页面渲染失败", error, info.componentStack);
  }

  componentDidUpdate(prev: Props) {
    if (prev.resetKey !== this.props.resetKey && this.state.error) {
      this.setState({ error: null });
    }
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="card pad" style={{ margin: 24 }}>
        <div className="row gap8" style={{ alignItems: "center", marginBottom: 10 }}>
          <span style={{ color: "var(--red)", display: "grid", placeItems: "center" }}>
            <Icon name="alert" size={20} />
          </span>
          <b style={{ fontSize: 15 }}>这个页面渲染出错</b>
        </div>
        <div className="faint" style={{ fontSize: 12.5, lineHeight: 1.7, marginBottom: 12 }}>
          应用的其他功能仍可正常使用。可以重试本页,或切换到其他页面后再回来。
        </div>
        <pre style={{
          margin: "0 0 14px", fontSize: 11.5, lineHeight: 1.6, maxHeight: 180,
          overflow: "auto", background: "rgba(5,8,16,.5)", padding: 10, borderRadius: 8,
          whiteSpace: "pre-wrap", wordBreak: "break-word", color: "var(--text-dim)",
        }}>{this.state.error.message || String(this.state.error)}</pre>
        <div className="row gap8">
          <button className="btn sm" onClick={() => this.setState({ error: null })}>
            <Icon name="refresh" size={13} /> 重试本页
          </button>
          <button className="btn ghost sm" onClick={() => window.location.reload()}>
            重新加载应用
          </button>
        </div>
      </div>
    );
  }
}

/**
 * 安装全局兜底:未捕获的异常与未处理的 Promise 拒绝统一记录并提示。
 * 返回卸载函数(测试用)。
 */
export function installGlobalErrorHandlers(
  notify: (kind: "error", text: string) => void,
): () => void {
  const onError = (e: ErrorEvent) => {
    console.error("[uncaught]", e.error ?? e.message);
    notify("error", `发生未处理的错误:${e.message || "未知错误"}`);
  };
  const onRejection = (e: PromiseRejectionEvent) => {
    console.error("[unhandledrejection]", e.reason);
    const text = e.reason instanceof Error ? e.reason.message : String(e.reason);
    notify("error", `后台操作失败:${text}`);
  };
  window.addEventListener("error", onError);
  window.addEventListener("unhandledrejection", onRejection);
  return () => {
    window.removeEventListener("error", onError);
    window.removeEventListener("unhandledrejection", onRejection);
  };
}
