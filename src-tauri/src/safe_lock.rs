//! 互斥量获取的统一入口。
//!
//! 本应用 release profile 为 `panic = "abort"`,一个线程 panic 后互斥量中毒,
//! 后续任何 `lock().unwrap()` 都会二次 panic 并让整个应用静默退出(托盘图标消失、
//! 用户无任何提示)。监控/计数/缓存这类数据在持锁线程 panic 后依然结构完整,
//! 因此中毒时取回内部值继续使用,而不是让整个进程崩溃。

use std::sync::{Mutex, MutexGuard};

/// 获取锁;中毒时沿用内部数据,不让中毒扩散成进程退出。
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
