// 诊断检测任务通用封装:统一各检测项 { run, running } 生命周期
// (置 running → 可选清旧结果 → 执行 → 成功写结果 / 失败弹 toast → 复位 running)
import { useEffect, useRef } from "react";
import { errMsg } from "../api";
import { useApp } from "../store/app";

export function useDiagTask<T>(opts: {
  running: boolean;
  setRunning: (v: boolean) => void;
  setResult: (v: T | null) => void;
  /** 部分检测开始前先清空旧结果(与原实现逐项保持一致) */
  clearFirst?: boolean;
  exec: () => Promise<T>;
}): { run: () => Promise<void>; running: boolean } {
  const { running, setRunning, setResult, clearFirst = false, exec } = opts;
  const activeRef = useRef(true);
  const inFlightRef = useRef(false);
  const requestRef = useRef(0);

  // A diagnostic call can outlive the page that started it. Do not let a
  // late response from an unmounted page overwrite the next page's result.
  useEffect(() => {
    activeRef.current = true;
    return () => {
      activeRef.current = false;
      inFlightRef.current = false;
      requestRef.current += 1;
      // 运行标志存放在全局 store 中;页面卸载后必须复位,否则返回诊断页时
      // 按钮会永久停在“检测中”。迟到的 finally 会因 request 不匹配而跳过。
      setRunning(false);
    };
  }, [setRunning]);

  const run = async () => {
    // React state updates are asynchronous; the ref closes the double-click
    // window before `running` is reflected by the next render.
    if (running || inFlightRef.current) return;
    inFlightRef.current = true;
    const request = ++requestRef.current;
    setRunning(true);
    if (clearFirst) setResult(null);
    try {
      const result = await exec();
      if (activeRef.current && request === requestRef.current) setResult(result);
    } catch (e) {
      if (activeRef.current && request === requestRef.current) {
        // A failed rerun must not leave a previous successful report visible.
        setResult(null);
        useApp.getState().toast("error", errMsg(e));
      }
    } finally {
      // 同一请求的收尾不受页面是否仍挂载影响:清零 in-flight 并复位全局运行标志。
      // 若期间已开始新请求(序号变化),旧请求不得覆盖新请求的状态。
      if (request === requestRef.current) {
        inFlightRef.current = false;
        setRunning(false);
      }
    }
  };

  return { run, running };
}
