//! 应用设置:JSON 持久化(原子写入),所有选项真实生效。

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::engine::model::ServerNodeInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    // ---------- 外观 ----------
    /// dark | light | system
    pub theme: String,
    /// 动画效果开关
    pub animations: bool,
    /// 动效强度 0.5 - 1.5
    pub anim_intensity: f32,
    /// 数据刷新频率(ms,监控图表)
    pub refresh_interval_ms: u32,

    // ---------- 测速 ----------
    /// auto | manual
    pub node_mode: String,
    /// 手动模式默认节点
    pub default_node_url: String,
    /// 测速时长(秒,5-30)
    pub test_duration_secs: u32,
    /// 精度模式(更长时长、更多并发与样本)
    pub precision_mode: bool,
    /// 下载并发连接数(1-16)
    pub down_concurrency: u32,
    /// 上传并发连接数(1-8)
    pub up_concurrency: u32,
    /// 自动保存测速记录
    pub autosave_history: bool,

    // ---------- 监控 ----------
    /// 后台网络监控开关
    pub monitor_enabled: bool,
    /// 状态栏显示实时流量
    pub show_traffic: bool,
    /// 开机自启动
    pub autostart: bool,
    /// 计划测速:按间隔自动执行后台测速
    pub auto_test_enabled: bool,
    /// 计划测速间隔(分钟,15-1440)
    pub auto_test_interval_minutes: u32,
    /// 桌面通知(计划测速结果 / 网络断开恢复)
    pub notify_events: bool,
    /// 延迟/丢包阈值告警
    pub alert_enabled: bool,
    /// 延迟告警阈值(ms)
    pub alert_threshold_ms: u32,
    /// 告警探测目标
    pub alert_target: String,
    /// 历史保留天数(0 = 永久)
    pub retention_days: u32,
    /// Prometheus /metrics 导出端点
    pub prometheus_enabled: bool,
    /// Prometheus 监听端口
    pub prometheus_port: u32,

    // ---------- 隐私 ----------
    /// 公网 IP 显示(未开启时不获取也不显示)
    pub show_public_ip: bool,
    /// 保存网络诊断历史
    pub save_diag_history: bool,
    /// 关闭窗口时最小化到托盘
    pub close_to_tray: bool,

    // ---------- 自定义节点 ----------
    pub custom_nodes: Vec<ServerNodeInfo>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            animations: true,
            anim_intensity: 1.0,
            refresh_interval_ms: 1000,
            node_mode: "auto".into(),
            default_node_url: String::new(),
            test_duration_secs: 10,
            precision_mode: false,
            down_concurrency: 6,
            up_concurrency: 4,
            autosave_history: true,
            monitor_enabled: true,
            show_traffic: true,
            autostart: false,
            auto_test_enabled: false,
            auto_test_interval_minutes: 60,
            notify_events: true,
            alert_enabled: false,
            alert_threshold_ms: 200,
            alert_target: "223.5.5.5".into(),
            retention_days: 90,
            prometheus_enabled: false,
            prometheus_port: 9876,
            show_public_ip: false,
            save_diag_history: true,
            close_to_tray: false,
            custom_nodes: Vec::new(),
        }
    }
}

pub struct SettingsManager {
    path: PathBuf,
    pub settings: Arc<RwLock<AppSettings>>,
}

impl SettingsManager {
    pub fn load(config_dir: &std::path::Path) -> Self {
        let path = config_dir.join("settings.json");
        let settings = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<AppSettings>(&text) {
                Ok(s) => s,
                Err(e) => {
                    // 损坏的设置文件:备份后重置,不再静默丢弃用户配置
                    log::warn!(
                        "settings.json 解析失败({e}),已备份为 settings.json.bak 并重置为默认设置"
                    );
                    let bak = config_dir.join("settings.json.bak");
                    let _ = std::fs::remove_file(&bak);
                    let _ = std::fs::rename(&path, &bak);
                    AppSettings::default()
                }
            },
            // 文件不存在是正常的首次运行
            Err(_) => AppSettings::default(),
        };
        Self {
            path,
            settings: Arc::new(RwLock::new(settings)),
        }
    }

    /// 原子写回(临时文件 + 重命名)。
    /// Windows 上 `rename` 会直接覆盖已存在的目标,因此不能先删原文件——
    /// 那会在"已删、未改名"的窗口里让进程崩溃丢失全部设置。
    pub async fn save(&self, settings: &AppSettings) -> Result<(), String> {
        let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
        let tmp = self.path.with_extension("json.tmp");
        tokio::fs::write(&tmp, json)
            .await
            .map_err(|e| e.to_string())?;
        tokio::fs::rename(&tmp, &self.path)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let s = AppSettings {
            theme: "light".into(),
            down_concurrency: 12,
            show_public_ip: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.theme, "light");
        assert_eq!(back.down_concurrency, 12);
        assert!(back.show_public_ip);
    }

    #[test]
    fn test_partial_json_uses_defaults() {
        // 前端只传部分字段时,其余字段走默认值(serde(default))
        let back: AppSettings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert_eq!(back.test_duration_secs, 10);
        assert_eq!(back.up_concurrency, 4);
        assert!(back.autosave_history);
    }

    #[test]
    fn test_legacy_unknown_fields_ignored() {
        let back: AppSettings =
            serde_json::from_str(r#"{"theme":"dark","someNewField":123}"#).unwrap();
        assert_eq!(back.theme, "dark");
    }

    #[test]
    fn test_corrupt_file_backed_up_and_reset() {
        let dir = std::env::temp_dir().join(format!("nv_settings_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, "{ not valid json").unwrap();
        let mgr = SettingsManager::load(&dir);
        assert!(
            dir.join("settings.json.bak").exists(),
            "损坏文件应备份为 .bak"
        );
        assert!(!path.exists(), "损坏文件应被移走");
        assert_eq!(
            mgr.settings.try_read().unwrap().theme,
            AppSettings::default().theme,
            "应回退到默认设置"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
