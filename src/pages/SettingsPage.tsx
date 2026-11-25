// 设置中心:外观 / 测速 / 监控 / 网络 / 隐私 / 关于 —— 所有选项真实生效
import { useCallback, useEffect, useState } from "react";
import { api, errMsg, onUpdaterProgress } from "../api";
import { useApp } from "../store/app";
import { useServers } from "../store/test";
import { fmtBytes, isCancelError } from "../lib/format";
import { ConfirmModal, Segmented, SettingRow } from "../components/ui";
import { Icon } from "../components/Icon";
import type { PrometheusStatus, ProxyRuntimeStatus, UpdaterInfo } from "../types";
import { useI18n, type LocaleSetting } from "../i18n";

export function SettingsPage() {
  const { setting: localeSetting, setSetting: setLocaleSetting, t } = useI18n();
  const toast = useApp((s) => s.toast);
  // 逐字段订阅:toast/网络状态等无关变化不触发整页重渲染
  const settings = useApp((s) => s.settings);
  const updateSettings = useApp((s) => s.updateSettings);
  const appInfo = useApp((s) => s.appInfo);
  const nodes = useServers((s) => s.nodes);
  const loadServers = useServers((s) => s.load);
  const [confirmClear, setConfirmClear] = useState(false);
  const [backupBusy, setBackupBusy] = useState<"export" | "import" | null>(null);
  const [proxyUrlDraft, setProxyUrlDraft] = useState("");
  const [proxyBypassDraft, setProxyBypassDraft] = useState("");
  const [proxyStatus, setProxyStatus] = useState<ProxyRuntimeStatus | null>(null);
  // 端点真实状态:设置里的开关只代表意图,绑定可能失败(端口占用)
  const [promStatus, setPromStatus] = useState<PrometheusStatus | null>(null);

  const refreshPromStatus = useCallback(async () => {
    try {
      setPromStatus(await api.prometheusStatus());
    } catch {
      setPromStatus(null); // 状态不可知时退回只显示设置意图
    }
  }, []);

  const refreshProxyStatus = useCallback(async () => {
    try {
      setProxyStatus(await api.proxyRuntimeStatus());
    } catch {
      setProxyStatus(null);
    }
  }, []);

  // 自动更新:检查结果与下载进度只在用户主动触发时获取
  const [update, setUpdate] = useState<UpdaterInfo | null>(null);
  const [updState, setUpdState] = useState<"idle" | "checking" | "downloading">("idle");
  const [updMessage, setUpdMessage] = useState<string | null>(null);
  const [downPct, setDownPct] = useState<number | null>(null);
  const [downBytes, setDownBytes] = useState<{ downloaded: number; total: number | null }>({ downloaded: 0, total: null });

  useEffect(() => {
    let alive = true;
    let un: (() => void) | null = null;
    onUpdaterProgress((p) => {
      if (!alive) return;
      if ("done" in p) {
        setDownPct(100);
      } else {
        setDownBytes({ downloaded: p.downloaded, total: p.total });
        if (p.total) setDownPct(Math.min(100, Math.round((p.downloaded / p.total) * 100)));
      }
    }).then((off) => {
      if (!alive) off();
      else un = off;
    });
    return () => {
      alive = false;
      un?.();
    };
  }, []);

  const checkUpdate = async () => {
    setUpdState("checking");
    setUpdMessage(null);
    setUpdate(null);
    try {
      const u = await api.updaterCheck();
      if (u) setUpdate(u);
      else setUpdMessage("已是最新版本");
    } catch (e) {
      setUpdMessage(`检查失败:${errMsg(e)}`);
    } finally {
      setUpdState((s) => (s === "checking" ? "idle" : s));
    }
  };

  const installUpdate = async () => {
    if (!update || updState !== "idle") return;
    setUpdState("downloading");
    setDownPct(0);
    setDownBytes({ downloaded: 0, total: null });
    setUpdMessage(null);
    try {
      // 安装成功时应用会被更新器重启,命令不会正常返回;返回即代表失败
      await api.updaterInstall(update.version);
      setUpdState("idle");
    } catch (e) {
      setUpdMessage(`更新失败:${errMsg(e)}`);
      setUpdState("idle");
    }
  };

  const exportSettings = async () => {
    setBackupBusy("export");
    try {
      const path = await api.settingsExport();
      toast("success", `设置已导出：${path}`);
    } catch (e) {
      if (!isCancelError(e)) toast("error", `设置导出失败：${errMsg(e)}`);
    } finally {
      setBackupBusy(null);
    }
  };

  const importSettings = async () => {
    setBackupBusy("import");
    try {
      const imported = await api.settingsImport();
      // IPC 返回的是后端已落盘的权威快照，直接替换本地 store，避免
      // 继续使用导入前的主题、监控和更新开关。
      useApp.setState({ settings: imported });
      useApp.getState().applyTheme();
      await loadServers();
      await refreshPromStatus();
      await refreshProxyStatus();
      toast("success", "设置已导入并生效");
    } catch (e) {
      if (!isCancelError(e)) toast("error", `设置导入失败：${errMsg(e)}`);
    } finally {
      setBackupBusy(null);
    }
  };

  useEffect(() => {
    loadServers();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    setProxyUrlDraft(settings?.proxy_url ?? "");
  }, [settings?.proxy_url]);

  useEffect(() => {
    setProxyBypassDraft(settings?.proxy_bypass ?? "");
  }, [settings?.proxy_bypass]);

  useEffect(() => {
    refreshPromStatus();
    // 绑定是异步的:设置变化后短延迟再查一次,避免"刚开就显示未监听"的假失败
    if (!settings?.prometheus_enabled) return;
    const t = window.setTimeout(refreshPromStatus, 400);
    return () => window.clearTimeout(t);
  }, [refreshPromStatus, settings?.prometheus_enabled, settings?.prometheus_port]);

  useEffect(() => {
    void refreshProxyStatus();
  }, [refreshProxyStatus, settings?.proxy_mode, settings?.proxy_url, settings?.proxy_bypass]);

  if (!settings) return null;
  const upd = updateSettings;

  return (
    <div>
      <div className="page-head">
        <div>
          <h1 className="page-title">
            <span className="icon-wrap"><Icon name="settings" size={22} /></span>
            设置与关于
          </h1>
          <div className="page-sub">所有设置即时保存并真实生效</div>
        </div>
      </div>

      <div className="settings-grid">
        <div className="settings-columns">
          <div className="settings-column">
            <section className="card pad settings-section settings-section--appearance">
              <div className="chart-title"><Icon name="eye" size={15} /> 通用设置与外观</div>
              <div className="chart-title mt12"><Icon name="globe" size={15} /> {t("language")}</div>
              <SettingRow className="set-row">
                <div>
                  <div className="label">{t("language")}</div>
                  <div className="desc">{t("languageHint")}</div>
                </div>
                <select className="select" value={localeSetting} onChange={(e) => setLocaleSetting(e.target.value as LocaleSetting)}>
                  <option value="auto">{t("auto")}</option>
                  <option value="zh-CN">简体中文</option>
                  <option value="en-US">English</option>
                </select>
              </SettingRow>
              <div className="chart-title mt12"><Icon name="eye" size={15} /> 外观</div>
              <SettingRow className="set-row">
                <div>
                  <div className="label">主题</div>
                  <div className="desc">跟随系统时自动切换明暗</div>
                </div>
                <Segmented
                  value={settings.theme}
                  onChange={(v) => upd({ theme: v })}
                  options={[
                    { value: "dark", label: "深色" },
                    { value: "light", label: "浅色" },
                    { value: "system", label: "跟随系统" },
                  ]}
                />
              </SettingRow>
              <SettingRow className="set-row">
                <div>
                  <div className="label">动画效果</div>
                  <div className="desc">关闭后所有过渡与动效立即结束,界面更省电</div>
                </div>
                <button className={`toggle ${settings.animations ? "on" : ""}`} onClick={() => upd({ animations: !settings.animations })} aria-label="动画效果" />
              </SettingRow>
              <SettingRow className="set-row">
                <div>
                  <div className="label">动效强度</div>
                  <div className="desc">{settings.anim_intensity.toFixed(1)}×(影响动画时长)</div>
                </div>
                <input
                  type="range" min={0.5} max={1.5} step={0.1} style={{ width: 150 }}
                  value={settings.anim_intensity}
                  onChange={(e) => upd({ anim_intensity: Number(e.target.value) })}
                  disabled={!settings.animations}
                />
              </SettingRow>
              <SettingRow className="set-row">
                <div>
                  <div className="label">数据刷新频率</div>
                  <div className="desc">监控图表的数据刷新间隔</div>
                </div>
                <select
                  className="select"
                  value={settings.refresh_interval_ms}
                  onChange={(e) => upd({ refresh_interval_ms: Number(e.target.value) })}
                >
                  <option value={500}>0.5 秒</option>
                  <option value={1000}>1 秒</option>
                  <option value={2000}>2 秒</option>
                  <option value={5000}>5 秒</option>
                </select>
              </SettingRow>
            </section>

            <section className="card pad settings-section settings-section--monitoring">
              <div className="chart-title"><Icon name="activity" size={15} /> 后台监控与系统行为</div>
              <SettingRow className="set-row">
                <div>
                  <div className="label">后台网络监控</div>
                  <div className="desc">每秒采样网卡流量;关闭后零开销</div>
                </div>
                <button
                  className={`toggle ${settings.monitor_enabled ? "on" : ""}`}
                  onClick={async () => {
                    const next = !settings.monitor_enabled;
                    await upd({ monitor_enabled: next });
                  }}
                  aria-label="后台监控"
                />
              </SettingRow>
              <SettingRow className="set-row">
                <div>
                  <div className="label">状态栏显示实时流量</div>
                  <div className="desc">在标题栏与状态栏显示当前收发速率</div>
                </div>
                <button className={`toggle ${settings.show_traffic ? "on" : ""}`} onClick={() => upd({ show_traffic: !settings.show_traffic })} aria-label="显示流量" />
              </SettingRow>
              <SettingRow className="set-row">
                <div>
                  <div className="label">开机自动启动</div>
                  <div className="desc">写入当前用户注册表 Run 项,可随时关闭</div>
                </div>
                <button
                  className={`toggle ${settings.autostart ? "on" : ""}`}
                  onClick={async () => {
                    // updateSettings 内部吞错并返回 false:失败时不弹成功提示
                    const ok = await upd({ autostart: !settings.autostart });
                    if (ok) toast("success", !settings.autostart ? "已开启开机自启" : "已关闭开机自启");
                    else toast("error", "自启动设置保存失败,请重试");
                  }}
                  aria-label="开机自启"
                />
              </SettingRow>
              <SettingRow className="set-row">
                <div>
                  <div className="label">关闭窗口时最小化到托盘</div>
                  <div className="desc">关闭按钮改为隐藏窗口,托盘可彻底退出</div>
                </div>
                <button className={`toggle ${settings.close_to_tray ? "on" : ""}`} onClick={() => upd({ close_to_tray: !settings.close_to_tray })} aria-label="关闭到托盘" />
              </SettingRow>
              <SettingRow className="set-row">
                <div>
                  <div className="label">计划测速</div>
                  <div className="desc">按设定间隔在后台自动执行真实测速并保存历史</div>
                </div>
                <button className={`toggle ${settings.auto_test_enabled ? "on" : ""}`} onClick={() => upd({ auto_test_enabled: !settings.auto_test_enabled })} aria-label="计划测速" />
              </SettingRow>
              {settings.auto_test_enabled && (
                <SettingRow className="set-row">
                  <div>
                    <div className="label">测速间隔</div>
                    <div className="desc">后台自动测速的执行频率(最小 15 分钟)</div>
                  </div>
                  <select
                    className="select"
                    value={settings.auto_test_interval_minutes}
                    onChange={(e) => upd({ auto_test_interval_minutes: Number(e.target.value) })}
                  >
                    <option value={30}>30 分钟</option>
                    <option value={60}>1 小时</option>
                    <option value={120}>2 小时</option>
                    <option value={360}>6 小时</option>
                    <option value={720}>12 小时</option>
                    <option value={1440}>24 小时</option>
                  </select>
                </SettingRow>
              )}
            </section>
          </div>
          <div className="settings-column">
            <section className="card pad settings-section settings-section--speed">
              <div className="chart-title"><Icon name="gauge" size={15} /> 测速</div>
              <SettingRow className="set-row">
                <div>
                  <div className="label">节点选择方式</div>
                  <div className="desc">自动优选会综合延迟、可用性与下载能力</div>
                </div>
                <Segmented
                  value={settings.node_mode}
                  onChange={(v) => upd({ node_mode: v })}
                  options={[
                    { value: "auto", label: "自动优选" },
                    { value: "manual", label: "手动指定" },
                  ]}
                />
              </SettingRow>
              {settings.node_mode === "manual" && (
                <SettingRow className="set-row">
                  <div>
                    <div className="label">默认节点</div>
                    <div className="desc">手动模式下使用的测速节点</div>
                  </div>
                  <select
                    className="select" style={{ maxWidth: 260 }}
                    value={settings.default_node_url}
                    onChange={(e) => upd({ default_node_url: e.target.value })}
                  >
                    {nodes.map((n) => (
                      <option key={n.id} value={n.base_url}>{n.name}</option>
                    ))}
                  </select>
                </SettingRow>
              )}
              <SettingRow className="set-row">
                <div>
                  <div className="label">测速持续时间</div>
                  <div className="desc">{settings.test_duration_secs} 秒(下载与上传各一段;NDT7 单阶段最多 10 秒)</div>
                </div>
                <input
                  type="range" min={5} max={30} step={1} style={{ width: 150 }}
                  value={settings.test_duration_secs}
                  onChange={(e) => upd({ test_duration_secs: Number(e.target.value) })}
                />
              </SettingRow>
              <SettingRow className="set-row">
                <div>
                  <div className="label">精度模式</div>
                  <div className="desc">更多并发与更密的采样,耗时略增</div>
                </div>
                <button className={`toggle ${settings.precision_mode ? "on" : ""}`} onClick={() => upd({ precision_mode: !settings.precision_mode })} aria-label="精度模式" />
              </SettingRow>
              <SettingRow className="set-row">
                <div>
                  <div className="label">下载并发连接</div>
                  <div className="desc">{settings.down_concurrency} 条(1-16,多流可对抗单流限速)</div>
                </div>
                <input
                  type="range" min={1} max={16} step={1} style={{ width: 150 }}
                  value={settings.down_concurrency}
                  onChange={(e) => upd({ down_concurrency: Number(e.target.value) })}
                />
              </SettingRow>
              <SettingRow className="set-row">
                <div>
                  <div className="label">上传并发连接</div>
                  <div className="desc">{settings.up_concurrency} 条(1-8)</div>
                </div>
                <input
                  type="range" min={1} max={8} step={1} style={{ width: 150 }}
                  value={settings.up_concurrency}
                  onChange={(e) => upd({ up_concurrency: Number(e.target.value) })}
                />
              </SettingRow>
              <SettingRow className="set-row">
                <div>
                  <div className="label">自动保存测速记录</div>
                  <div className="desc">完成 / 取消 / 失败的测试均如实入库</div>
                </div>
                <button className={`toggle ${settings.autosave_history ? "on" : ""}`} onClick={() => upd({ autosave_history: !settings.autosave_history })} aria-label="自动保存" />
              </SettingRow>
            </section>

            <section className="card pad settings-section settings-section--alerts">
              <div className="chart-title"><Icon name="activity" size={15} /> 通知、告警与指标</div>
              <SettingRow className="set-row">
                <div>
                  <div className="label">桌面通知</div>
                  <div className="desc">计划测速结果、网络断开/恢复时弹出系统通知</div>
                </div>
                <button className={`toggle ${settings.notify_events ? "on" : ""}`} onClick={() => upd({ notify_events: !settings.notify_events })} aria-label="桌面通知" />
              </SettingRow>
              <SettingRow className="set-row">
                <div>
                  <div className="label">延迟/丢包阈值告警</div>
                  <div className="desc">每 10s 真实探测目标,均值超阈值或连续 3 次超时则通知(10 分钟冷却)</div>
                </div>
                <button className={`toggle ${settings.alert_enabled ? "on" : ""}`} onClick={() => upd({ alert_enabled: !settings.alert_enabled })} aria-label="阈值告警" />
              </SettingRow>
              {settings.alert_enabled && (
                <SettingRow className="set-row">
                  <div>
                    <div className="label">告警阈值 / 目标</div>
                    <div className="desc">平均延迟超过阈值触发;目标建议用低延迟稳定地址</div>
                  </div>
                  <div className="row gap8">
                    <input type="number" min={50} max={1000} step={10} className="text-input"
                           style={{ width: 100 }} value={settings.alert_threshold_ms}
                           onChange={(e) => upd({ alert_threshold_ms: Number(e.target.value) })}
                           aria-label="延迟阈值" />
                    <input className="text-input" style={{ width: 150 }} value={settings.alert_target}
                           onChange={(e) => upd({ alert_target: e.target.value })} aria-label="告警目标" />
                  </div>
                </SettingRow>
              )}
              <SettingRow className="set-row">
                <div>
                  <div className="label">Prometheus / Grafana 指标导出</div>
                  <div className="desc">
                    仅监听 127.0.0.1,提供 /metrics 文本格式
                    {settings.prometheus_enabled && promStatus?.listening && (
                      <span className="mono" style={{ color: "var(--cyan)" }}> · {promStatus.url}</span>
                    )}
                    {settings.prometheus_enabled && promStatus && !promStatus.listening && (
                      <span style={{ color: "var(--red)" }}>
                        {" "}· {promStatus.error ?? "端点未在监听"}
                      </span>
                    )}
                  </div>
                </div>
                <button className={`toggle ${settings.prometheus_enabled ? "on" : ""}`}
                        onClick={() => upd({ prometheus_enabled: !settings.prometheus_enabled })}
                        aria-label="指标导出" />
              </SettingRow>
              {settings.prometheus_enabled && (
                <SettingRow className="set-row">
                  <div>
                    <div className="label">导出端口</div>
                    <div className="desc">1024-65535,修改后自动重启端点</div>
                  </div>
                  <input type="number" min={1024} max={65535} className="text-input"
                         style={{ width: 120 }} value={settings.prometheus_port}
                         onChange={(e) => upd({ prometheus_port: Number(e.target.value) })}
                         aria-label="导出端口" />
                </SettingRow>
              )}
            </section>
          </div>
        </div>
        <section className="card pad settings-section settings-section--network">
          <div className="chart-title"><Icon name="wifi" size={15} /> 网络与代理</div>
          <SettingRow className="set-row">
            <div>
              <div className="label">代理模式</div>
              <div className="desc">自动读取环境变量和 Windows Internet 设置；直连模式不会使用系统代理</div>
            </div>
            <select
              className="select"
              value={settings.proxy_mode}
              onChange={(e) => upd({ proxy_mode: e.target.value as "auto" | "direct" | "manual" })}
            >
              <option value="auto">自动代理</option>
              <option value="direct">直连</option>
              <option value="manual">手动代理</option>
            </select>
          </SettingRow>
          {settings.proxy_mode === "manual" && (
            <SettingRow className="set-row">
              <div>
                <div className="label">代理地址</div>
                <div className="desc">支持 HTTP/HTTPS 代理，例如 127.0.0.1:7890；离开输入框后保存</div>
              </div>
              <input
                className="text-input settings-proxy-input"
                value={proxyUrlDraft}
                onChange={(e) => setProxyUrlDraft(e.target.value)}
                onBlur={() => {
                  if (proxyUrlDraft !== settings.proxy_url) void upd({ proxy_url: proxyUrlDraft });
                }}
                placeholder="http://127.0.0.1:7890"
                aria-label="手动代理地址"
                spellCheck={false}
              />
            </SettingRow>
          )}
          <SettingRow className="set-row">
            <div>
              <div className="label">代理绕过列表</div>
              <div className="desc">可填写主机名或 IP，使用逗号、分号或空格分隔；PAC/WPAD 暂不支持</div>
            </div>
            <input
              className="text-input settings-proxy-input"
              value={proxyBypassDraft}
              onChange={(e) => setProxyBypassDraft(e.target.value)}
              onBlur={() => {
                if (proxyBypassDraft !== settings.proxy_bypass) void upd({ proxy_bypass: proxyBypassDraft });
              }}
              placeholder="localhost, 127.0.0.1"
              aria-label="代理绕过列表"
              spellCheck={false}
            />
          </SettingRow>
          {proxyStatus?.warning && (
            <div className="test-error settings-proxy-warning" role="alert">
              {proxyStatus.warning} 当前自动代理规则无法测速或诊断；应用不会静默改为直连。
            </div>
          )}
          {!proxyStatus?.warning && proxyStatus?.uses_proxy && !proxyStatus.ndt7_websocket_available && (
            <div className="settings-proxy-notice" role="status">
              当前代理下 HTTP 测速可用；NDT7 WebSocket 暂不可用，自动测速会跳过该协议，不会绕过代理直连。
            </div>
          )}
        </section>

        <section className="card pad settings-section settings-section--privacy">
          <div className="chart-title"><Icon name="shield" size={15} /> 隐私与数据</div>
          <SettingRow className="set-row">
            <div>
              <div className="label">显示公网 IP 与 ISP</div>
              <div className="desc">关闭时不向任何服务查询 IP;开启后仅查询测速同源服务</div>
            </div>
            <button className={`toggle ${settings.show_public_ip ? "on" : ""}`} onClick={() => upd({ show_public_ip: !settings.show_public_ip })} aria-label="公网 IP" />
          </SettingRow>
          <SettingRow className="set-row">
            <div>
              <div className="label">保存诊断数据</div>
              <div className="desc">开启后把诊断结果保存到本机历史数据库，按历史保留策略清理</div>
            </div>
            <button className={`toggle ${settings.save_diag_history ? "on" : ""}`} onClick={() => upd({ save_diag_history: !settings.save_diag_history })} aria-label="保存诊断" />
          </SettingRow>
          <SettingRow className="set-row">
            <div>
              <div className="label">数据与设置备份</div>
              <div className="desc">导出测速历史，或把当前设置保存为可迁移的 JSON 备份</div>
            </div>
            <div className="row gap8 wrap">
              <button className="btn sm" disabled={backupBusy !== null} onClick={async () => {
                try { toast("success", `已导出:${await api.historyExport("csv")}`); }
                catch (e) { if (!isCancelError(e)) toast("error", String(e)); }
              }}>历史 CSV</button>
              <button className="btn sm" disabled={backupBusy !== null} onClick={async () => {
                try { toast("success", `已导出:${await api.historyExport("json")}`); }
                catch (e) { if (!isCancelError(e)) toast("error", String(e)); }
              }}>历史 JSON</button>
              <button className="btn sm" disabled={backupBusy !== null} onClick={exportSettings}>
                {backupBusy === "export" ? "导出中…" : "备份设置"}
              </button>
              <button className="btn sm" disabled={backupBusy !== null} onClick={importSettings}>
                {backupBusy === "import" ? "导入中…" : "恢复设置"}
              </button>
            </div>
          </SettingRow>
          <SettingRow className="set-row">
            <div>
              <div className="label">历史保留策略</div>
              <div className="desc">自动删除超过保留期的测速记录(启动时清理)</div>
            </div>
            <select className="select" value={settings.retention_days}
                    onChange={(e) => upd({ retention_days: Number(e.target.value) })}>
              <option value={0}>永久保留</option>
              <option value={30}>30 天</option>
              <option value={90}>90 天</option>
              <option value={180}>180 天</option>
              <option value={365}>1 年</option>
            </select>
          </SettingRow>
          <SettingRow className="set-row">
            <div>
              <div className="label">清空历史记录</div>
              <div className="desc">永久删除本机全部测速记录</div>
            </div>
            <button className="btn sm danger" onClick={() => setConfirmClear(true)}><Icon name="trash" size={13} /> 清空</button>
          </SettingRow>
        </section>

        <section className="card pad about-card settings-section settings-section--about">
          <div className="chart-title"><Icon name="info" size={15} /> 关于</div>
          <div className="row gap12 mt8">
            <div className="brand-logo" style={{ width: 44, height: 44 }}>
              <Icon name="gauge" size={24} />
            </div>
            <div>
              <div style={{ fontWeight: 700, fontSize: 15 }}>NetVision Pro</div>
              <div className="faint" style={{ fontSize: 12 }}>
                版本 {appInfo?.version ?? "1.11.0"} · Windows ({appInfo?.os ?? "windows"})
              </div>
              <div className="faint" style={{ fontSize: 12 }}>
                专业网络测速与诊断 · 真实测量,拒绝模拟数据
              </div>
            </div>
          </div>
          <div className="faint mt12" style={{ fontSize: 11.5, lineHeight: 1.9 }}>
            数据目录:{appInfo?.data_dir ?? "-"}
            <br />
            设置文件:{appInfo?.settings_path ?? "-"}
            <br />
            测速协议:LibreSpeed(HTTP)· Cloudflare Speed· M-Lab NDT7(WebSocket)
            <br />
            说明:测速结果受服务器性能、Wi-Fi 信号与网卡协商速率影响;本软件不会把网卡标称速度当作宽带速率展示。
          </div>
          <div className="row gap8 mt12 wrap" style={{ alignItems: "center" }}>
            <button className="btn sm" onClick={checkUpdate} disabled={updState !== "idle"}>
              <Icon name="refresh" size={13} />
              {updState === "checking" ? "检查中…" : "检查更新"}
            </button>
            {update && updState !== "downloading" && (
              <button
                className="btn primary sm"
                onClick={installUpdate}
                title={update.notes ?? undefined}
              >
                下载并安装 v{update.version}
              </button>
            )}
            {updState === "downloading" && (
              <span className="faint" style={{ fontSize: 12 }}>
                正在下载更新…{downPct != null ? `${downPct}%` : ""}
                {downBytes.downloaded > 0 && ` · ${fmtBytes(downBytes.downloaded)}${downBytes.total ? ` / ${fmtBytes(downBytes.total)}` : ""}`}
              </span>
            )}
            {updMessage && (
              <span className="faint" style={{ fontSize: 11.5 }}>{updMessage}</span>
            )}
          </div>
          {update && (
            <div className="faint mt8" style={{ fontSize: 11.5, lineHeight: 1.7, whiteSpace: "pre-wrap", maxWidth: 760 }}>
              {update.pub_date && <>发布时间：{new Date(update.pub_date).toLocaleString()}<br /></>}
              {update.notes || "该版本没有提供更新说明。"}
            </div>
          )}
          <SettingRow className="set-row mt12">
            <div>
              <div className="label">自动检查稳定版更新</div>
              <div className="desc">启动后按间隔检查 GitHub Releases；网络失败不会影响应用启动</div>
            </div>
            <button
              className={`toggle ${settings.auto_update_enabled ? "on" : ""}`}
              onClick={() => upd({ auto_update_enabled: !settings.auto_update_enabled })}
              aria-label="自动检查更新"
            />
          </SettingRow>
          <SettingRow className="set-row">
            <div>
              <div className="label">自动检查间隔</div>
              <div className="desc">仅使用稳定版 Release；Beta/Dev 通道暂未发布</div>
            </div>
            <select
              className="select"
              value={settings.update_check_interval_hours}
              onChange={(e) => upd({ update_check_interval_hours: Number(e.target.value) })}
              disabled={!settings.auto_update_enabled}
              aria-label="自动检查间隔"
            >
              <option value={6}>每 6 小时</option>
              <option value={24}>每天</option>
              <option value={72}>每 3 天</option>
              <option value={168}>每周</option>
            </select>
          </SettingRow>
        </section></div>

      {confirmClear && (
        <ConfirmModal
          title="清空历史记录"
          text="将永久删除全部测速记录,该操作不可恢复。"
          danger
          onOk={async () => {
            try {
              await api.historyClear();
              toast("success", "历史记录已清空");
            } catch (e) {
              toast("error", String(e));
            }
          }}
          onClose={() => setConfirmClear(false)}
        />
      )}
    </div>
  );
}
