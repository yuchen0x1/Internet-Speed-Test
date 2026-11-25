// @vitest-environment jsdom
import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useDiagTask } from "./useDiagTask";

interface HarnessProps {
  exec: () => Promise<string>;
  onState: (state: { result: string | null; running: boolean; run: () => Promise<void> }) => void;
}

function Harness({ exec, onState }: HarnessProps) {
  const [result, setResult] = useState<string | null>("previous result");
  const [running, setRunning] = useState(false);
  const task = useDiagTask({ running, setRunning, setResult, exec });
  onState({ result, running, run: task.run });
  return null;
}

describe("useDiagTask", () => {
  let root: Root | null = null;
  const host = document.createElement("div");
  document.body.append(host);

  afterEach(() => {
    if (root) {
      act(() => root?.unmount());
      root = null;
    }
    host.replaceChildren();
  });

  it("rejects an immediate duplicate call before React commits running state", async () => {
    let resolve!: (value: string) => void;
    const exec = vi.fn(() => new Promise<string>((r) => { resolve = r; }));
    let state!: { result: string | null; running: boolean; run: () => Promise<void> };
    root = createRoot(host);
    await act(async () => {
      root?.render(<Harness exec={exec} onState={(next) => { state = next; }} />);
    });

    await act(async () => {
      const first = state.run();
      const duplicate = state.run();
      expect(exec).toHaveBeenCalledTimes(1);
      resolve("fresh result");
      await Promise.all([first, duplicate]);
    });

    expect(state).toMatchObject({ result: "fresh result", running: false });
  });

  it("clears a previous successful result when a rerun fails", async () => {
    const exec = vi.fn().mockRejectedValue(new Error("connection failed"));
    let state!: { result: string | null; running: boolean; run: () => Promise<void> };
    root = createRoot(host);
    await act(async () => {
      root?.render(<Harness exec={exec} onState={(next) => { state = next; }} />);
    });
    expect(state.result).toBe("previous result");

    await act(async () => {
      await state.run();
    });

    expect(state).toMatchObject({ result: null, running: false });
  });
});
