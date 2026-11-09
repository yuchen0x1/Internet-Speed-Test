//! 连续 Ping 监测(gping 式):1s 间隔真实 ICMP,事件流驱动前端实时点图。
//!
//! - 使用 IcmpSendEcho 系统调用(免管理员、无子进程)
//! - 全局单例:新目标自动替换旧监测;页面关闭时前端应调用 stop
//! - 超时/失败也如实上报(rtt=null),前端画为丢失点

use std::sync::Mutex;

use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::diagnostics;

#[derive(Default)]
pub struct PingMonState {
    token: Mutex<Option<CancellationToken>>,
    host: Mutex<Option<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PingMonTick {
    pub ts_ms: u64,
    pub rtt_ms: Option<f64>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl PingMonState {
    pub fn start(
        &self,
        app: tauri::AppHandle,
        host: String,
    ) -> Result<(), String> {
        use tauri::Emitter as _;
        // 先校验输入再停旧任务:非法主机名不应中断正在运行的监测
        if !diagnostics::valid_host(&host) {
            return Err("无效的主机名".into());
        }
        self.stop();
        let token = CancellationToken::new();
        *self.host.lock().unwrap() = Some(host.clone());
        let child = token.clone();
        *self.token.lock().unwrap() = Some(token);
        tauri::async_runtime::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_millis(1000));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    _ = child.cancelled() => return,
                    _ = tick.tick() => {
                        let p = diagnostics::icmp_ping(&host, 1, 1200).await;
                        let rtt = p.rtts_ms.first().copied();
                        let payload = PingMonTick { ts_ms: now_ms(), rtt_ms: rtt };
                        let _ = app.emit("pingmon://tick", &payload);
                    }
                }
            }
        });
        Ok(())
    }

    pub fn stop(&self) {
        if let Some(t) = self.token.lock().unwrap().take() {
            t.cancel();
        }
        *self.host.lock().unwrap() = None;
    }

    pub fn current_host(&self) -> Option<String> {
        self.host.lock().unwrap().clone()
    }
}
