// 诊断检测任务通用封装:统一各检测项 { run, running } 生命周期
// (置 running → 可选清旧结果 → 执行 → 成功写结果 / 失败弹 toast → 复位 running)
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

  const run = async () => {
    setRunning(true);
    if (clearFirst) setResult(null);
    try {
      setResult(await exec());
    } catch (e) {
      useApp.getState().toast("error", errMsg(e));
    } finally {
      setRunning(false);
    }
  };

  return { run, running };
}
