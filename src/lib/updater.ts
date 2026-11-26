export function isUpdateCheckDue(last: number, intervalMs: number, now = Date.now()): boolean {
  return !Number.isFinite(last) || last <= 0 || last > now || now - last >= intervalMs;
}
