/** Small orchestration helpers shared by the diagnostics page and tests. */

export interface DiagnosticBatchTask {
  id: string;
  label: string;
  run: () => Promise<void>;
}

export interface DiagnosticBatchResult {
  id: string;
  label: string;
  ok: boolean;
  error?: string;
}

export interface DiagnosticBatchProgress {
  completed: number;
  total: number;
  results: DiagnosticBatchResult[];
  running?: boolean;
}

/**
 * Run independent diagnostic calls with a bounded number of in-flight tasks.
 * A single failure is recorded and does not prevent the remaining checks from running.
 */
export async function runDiagnosticBatch(
  tasks: DiagnosticBatchTask[],
  concurrency = 3,
  onProgress?: (progress: DiagnosticBatchProgress) => void,
): Promise<DiagnosticBatchResult[]> {
  if (tasks.length === 0) {
    onProgress?.({ completed: 0, total: 0, results: [] });
    return [];
  }

  const results: DiagnosticBatchResult[] = [];
  let next = 0;
  let completed = 0;
  const limit = Math.max(1, Math.min(Math.floor(concurrency) || 1, tasks.length));

  const worker = async () => {
    while (true) {
      const index = next++;
      const task = tasks[index];
      if (!task) return;
      try {
        await task.run();
        results[index] = { id: task.id, label: task.label, ok: true };
      } catch (error) {
        results[index] = {
          id: task.id,
          label: task.label,
          ok: false,
          error: error instanceof Error ? error.message : String(error),
        };
      } finally {
        completed += 1;
        onProgress?.({ completed, total: tasks.length, results: results.filter(Boolean) });
      }
    }
  };

  await Promise.all(Array.from({ length: limit }, () => worker()));
  return results.filter(Boolean);
}
