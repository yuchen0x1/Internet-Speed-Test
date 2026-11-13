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

    // ---------- 更新 ----------
    /// 启动后自动检查稳定版更新
    pub auto_update_enabled: bool,
    /// 自动检查间隔(小时,6-168)
    pub update_check_interval_hours: u32,

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
            auto_update_enabled: true,
            update_check_interval_hours: 24,
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
                    // 保留已有备份，多次损坏也不覆盖用户上一次的恢复点。
                    let mut bak = config_dir.join("settings.json.bak");
                    let mut suffix = 1u64;
                    while bak.exists() {
                        bak = config_dir.join(format!("settings.json.bak.{suffix}"));
                        suffix += 1;
                    }
                    match std::fs::rename(&path, &bak) {
                        Ok(()) => log::warn!("settings.json 解析失败({e})，已备份并使用默认设置"),
                        Err(backup_error) => log::warn!("settings.json 解析失败({e})，备份失败({backup_error})；原文件保留，使用默认设置"),
                    }
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

    /// 自启动变更失败不落盘；落盘失败则补偿系统状态。调用方持有设置写锁。
    pub async fn save_with_autostart(
        &self,
        settings: &AppSettings,
        previous: Option<bool>,
        mut apply: impl FnMut(bool) -> Result<(), String>,
    ) -> Result<(), String> {
        let changed = previous.is_some_and(|old| old != settings.autostart);
        if changed {
            if let Err(error) = apply(settings.autostart) {
                return match apply(previous.unwrap()) {
                    Ok(()) => Err(format!("自启动设置失败，应用设置未保存：{error}")),
                    Err(rollback) => Err(format!(
                        "自启动设置失败：{error}；系统状态恢复失败：{rollback}"
                    )),
                };
            }
        }
        if let Err(error) = self.save(settings).await {
            if changed {
                if let Err(rollback) = apply(previous.unwrap()) {
                    return Err(format!(
                        "设置保存失败：{error}；自启动状态恢复失败：{rollback}"
                    ));
                }
            }
            return Err(error);
        }
        Ok(())
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

/// HTTP 只允许已保存、且用户明确授权的自定义节点。
pub fn validate_manual_node(settings: &AppSettings, url: &str) -> Result<String, String> {
    let normalized = crate::engine::util::validate_server_url(url, true)?;
    let allow_http = settings
        .custom_nodes
        .iter()
        .any(|n| n.allow_http && n.base_url.trim_end_matches('/') == normalized);
    crate::engine::util::validate_server_url(&normalized, allow_http)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated_manager() -> SettingsManager {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "recheck-settings-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&dir).unwrap();
        SettingsManager::load(&dir)
    }

    #[tokio::test]
    async fn test_autostart_failure_never_commits_settings() {
        let manager = isolated_manager();
        manager.save(&AppSettings::default()).await.unwrap();
        let original = std::fs::read(manager.path()).unwrap();
        let next = AppSettings {
            autostart: true,
            theme: "light".into(),
            ..Default::default()
        };
        let mut system_enabled = false;
        let error = manager
            .save_with_autostart(&next, Some(false), |enabled| {
                system_enabled = enabled;
                if enabled {
                    Err("registry unavailable".into())
                } else {
                    Ok(())
                }
            })
            .await
            .unwrap_err();
        assert!(error.contains("未保存"));
        assert!(!system_enabled);
        assert_eq!(std::fs::read(manager.path()).unwrap(), original);
    }

    #[tokio::test]
    async fn test_disk_failure_restores_autostart_and_keeps_previous_file() {
        let manager = isolated_manager();
        manager.save(&AppSettings::default()).await.unwrap();
        let original = std::fs::read(manager.path()).unwrap();
        std::fs::create_dir(manager.path().with_extension("json.tmp")).unwrap();
        let next = AppSettings {
            autostart: true,
            ..Default::default()
        };
        let mut calls = Vec::new();
        assert!(manager
            .save_with_autostart(&next, Some(false), |enabled| {
                calls.push(enabled);
                Ok(())
            })
            .await
            .is_err());
        assert_eq!(calls, vec![true, false]);
        assert_eq!(std::fs::read(manager.path()).unwrap(), original);
    }

    #[tokio::test]
    async fn test_other_settings_do_not_touch_system_autostart() {
        let manager = isolated_manager();
        let next = AppSettings {
            theme: "light".into(),
            ..Default::default()
        };
        manager
            .save_with_autostart(&next, None, |_| panic!("unrelated save touched autostart"))
            .await
            .unwrap();
        let saved: AppSettings =
            serde_json::from_slice(&std::fs::read(manager.path()).unwrap()).unwrap();
        assert_eq!(saved.theme, "light");
    }

    #[test]
    fn test_http_manual_node_requires_saved_explicit_consent() {
        let mut settings = AppSettings::default();
        assert!(validate_manual_node(&settings, "http://localhost:8080/").is_err());
        settings.custom_nodes.push(ServerNodeInfo {
            base_url: "http://localhost:8080".into(),
            allow_http: true,
            ..Default::default()
        });
        assert_eq!(
            validate_manual_node(&settings, "http://localhost:8080/").unwrap(),
            "http://localhost:8080"
        );
        assert!(validate_manual_node(&settings, "http://other.example").is_err());
        settings.custom_nodes[0].allow_http = false;
        assert!(validate_manual_node(&settings, "http://localhost:8080").is_err());
        assert!(validate_manual_node(&settings, "https://example.com/").is_ok());
    }

    #[test]
    fn test_repeated_corruption_keeps_existing_backup() {
        let manager = isolated_manager();
        let dir = manager.path().parent().unwrap();
        std::fs::write(dir.join("settings.json.bak"), "previous recovery point").unwrap();
        std::fs::write(manager.path(), "{ broken config").unwrap();
        let _ = SettingsManager::load(dir);
        assert_eq!(
            std::fs::read_to_string(dir.join("settings.json.bak")).unwrap(),
            "previous recovery point"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("settings.json.bak.1")).unwrap(),
            "{ broken config"
        );
    }

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
        assert!(back.auto_update_enabled);
        assert_eq!(back.update_check_interval_hours, 24);
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
