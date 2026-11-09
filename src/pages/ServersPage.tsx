// 测速节点管理:列表/探测/收藏/切换/自定义节点
import { useEffect, useState } from "react";
import { api } from "../api";
import { useApp } from "../store/app";
import { useServers, useTest } from "../store/test";
import { Modal } from "../components/ui";
import { NodeRadar } from "../components/NodeRadar";
import { Icon } from "../components/Icon";
import type { ServerNodeInfo } from "../types";

const PROVIDER_TEXT: Record<string, string> = {
  librespeed: "LibreSpeed",
  cloudflare: "Cloudflare",
  ndt7: "M-Lab NDT7",
  custom: "自定义",
};

/** 自定义节点沿用 LibreSpeed 官方后端的默认端点路径 */
const CUSTOM_ENDPOINTS = {
  dl_url: "garbage.php",
  ul_url: "empty.php",
  ping_url: "empty.php",
  getip_url: "getIP.php",
} as const;

export function ServersPage() {
  const toast = useApp((s) => s.toast);
  const settings = useApp((s) => s.settings);
  const updateSettings = useApp((s) => s.updateSettings);
  const { nodes, loading, probing, load, refresh } = useServers();
  const currentNode = useTest((s) => s.node);
  const [addOpen, setAddOpen] = useState(false);

  useEffect(() => {
    load();
  }, [load]);

  const activeUrl = currentNode?.base_url ?? settings?.default_node_url ?? "";

  const useNode = async (n: ServerNodeInfo) => {
    try {
      await api.serversSetManual(n.base_url);
      await updateSettings({ node_mode: "manual", default_node_url: n.base_url });
      toast("success", `已切换手动节点:${n.name}`);
    } catch (e) {
      toast("error", String(e));
    }
  };

  const useAuto = async () => {
    try {
      await api.serversSetAuto();
      await updateSettings({ node_mode: "auto" });
      toast("success", "已切换为自动优选(延迟 + 可用性 + 下载能力综合评估)");
    } catch (e) {
      toast("error", String(e));
    }
  };

  const toggleFav = async (n: ServerNodeInfo) => {
    try {
      await api.serversSetFavorite(n.base_url, n.name, !n.favorite);
      load(true);
    } catch (e) {
      toast("error", String(e));
    }
  };

  const removeCustom = async (n: ServerNodeInfo) => {
    try {
      await api.serversRemoveCustom(n.base_url);
      toast("success", "已删除自定义节点");
      load(true);
    } catch (e) {
      toast("error", String(e));
    }
  };

  // 收藏优先,其次延迟
  const sorted = [...nodes].sort((a, b) => {
    if (a.favorite !== b.favorite) return a.favorite ? -1 : 1;
    const al = a.latency_ms ?? 9_999;
    const bl = b.latency_ms ?? 9_999;
    return al - bl;
  });

  return (
    <div>
      <div className="page-head">
        <div>
          <h1 className="page-title">
            <span className="icon-wrap"><Icon name="server" size={22} /></span>
            测速节点
          </h1>
          <div className="page-sub">
            节点来自 LibreSpeed 官方列表、Cloudflare 边缘网络与 M-Lab NDT7 Locate 服务发现
            {probing ? " · 正在探测…" : ""}
          </div>
        </div>
        <div className="row gap8">
          <button className={`btn ${settings?.node_mode === "auto" ? "primary" : ""}`} onClick={useAuto} disabled={settings?.node_mode === "auto"}>
            <Icon name="zap" size={14} /> 自动优选
          </button>
          <button className="btn" onClick={refresh} disabled={probing}>
            <Icon name="refresh" size={14} className={probing ? "spin" : ""} />
            {probing ? "探测中" : "重新检测节点"}
          </button>
          <button className="btn" onClick={() => setAddOpen(true)}><Icon name="plus" size={14} /> 添加节点</button>
        </div>
      </div>

      <div className="card pad mb16">
        <div className="chart-title"><Icon name="target" size={15} /> 节点延迟雷达(探测后显示,同心环 = 延迟档位)</div>
        <NodeRadar nodes={nodes} size={280} />
      </div>

      <div className="card" style={{ padding: 0 }}>
        <div className="table-wrap" style={{ maxHeight: "60vh", overflowY: "auto" }}>
          <table className="table">
            <thead>
              <tr>
                <th style={{ width: 36 }}></th>
                <th>节点</th>
                <th>提供商</th>
                <th>延迟</th>
                <th>状态</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {sorted.map((n) => {
                const active = activeUrl === n.base_url;
                const lat = n.latency_ms;
                return (
                  <tr key={n.id}>
                    <td>
                      <button className="btn ghost sm" onClick={() => toggleFav(n)} title={n.favorite ? "取消收藏" : "收藏"}>
                        <Icon name={n.favorite ? "star-filled" : "star"} size={15} />
                      </button>
                    </td>
                    <td>
                      <div className="row gap8">
                        <b style={{ fontSize: 13 }}>{n.name}</b>
                        {active && settings?.node_mode === "manual" && <span className="chip cyan">当前</span>}
                        {n.allow_http && <span className="chip amber">HTTP</span>}
                      </div>
                      {n.location && <div className="faint" style={{ fontSize: 11.5 }}>{n.location}</div>}
                    </td>
                    <td><span className={`chip ${n.provider === "cloudflare" ? "purple" : n.provider === "custom" ? "amber" : n.provider === "ndt7" ? "cyan" : ""}`}>{PROVIDER_TEXT[n.provider] ?? n.provider}</span></td>
                    <td className="num">{lat != null ? `${lat.toFixed(1)} ms` : "—"}</td>
                    <td>
                      {n.alive == null ? (
                        <span className="chip">未探测</span>
                      ) : n.alive ? (
                        <span className="chip green"><span className="status-dot ok" style={{ width: 6, height: 6 }} /> 可用</span>
                      ) : (
                        <span className="chip red">不可达</span>
                      )}
                    </td>
                    <td>
                      <div className="row gap4">
                        <button
                          className={`btn sm ${settings?.node_mode === "manual" && settings.default_node_url === n.base_url ? "primary" : ""}`}
                          onClick={() => useNode(n)}
                          disabled={settings?.node_mode === "manual" && settings.default_node_url === n.base_url}
                        >
                          使用
                        </button>
                        {n.provider === "custom" && (
                          <button className="btn ghost sm" onClick={() => removeCustom(n)} title="删除"><Icon name="trash" size={13} /></button>
                        )}
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {nodes.length === 0 && (
            <div className="empty">
              <Icon name="server" size={34} />
              <span>{loading || probing ? "正在获取节点列表…" : "节点列表为空"}</span>
            </div>
          )}
        </div>
      </div>

      <div className="faint mt12" style={{ fontSize: 11.5, lineHeight: 1.8 }}>
        自动优选标准:并发探测全部节点的真实 HTTP 延迟与可用性,对延迟最优的 4 个候选执行 2.5s 下载能力探测,按 延迟 65% + 下载能力 35% 综合评分。地理距离不参与评分 —— 网络路径质量才是决定因素。
      </div>

      {addOpen && <AddNodeModal onClose={() => setAddOpen(false)} />}
    </div>
  );
}

function AddNodeModal({ onClose }: { onClose: () => void }) {
  const toast = useApp((s) => s.toast);
  const loadServers = useServers((s) => s.load);
  const [url, setUrl] = useState("https://");
  const [name, setName] = useState("");
  const [allowHttp, setAllowHttp] = useState(false);
  const [busy, setBusy] = useState(false);

  const add = async () => {
    setBusy(true);
    try {
      // serversAddCustom 只要求 Partial<ServerNodeInfo>,无需整段类型断言
      await api.serversAddCustom({
        base_url: url.trim(),
        name: name.trim(),
        provider: "custom",
        allow_http: allowHttp,
        ...CUSTOM_ENDPOINTS,
      });
      toast("success", "节点已添加");
      loadServers(true);
      onClose();
    } catch (e) {
      toast("error", String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title="添加 LibreSpeed 兼容节点"
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>取消</button>
          <button className="btn primary" onClick={add} disabled={busy || !url.startsWith("http")}>{busy ? "添加中…" : "添加节点"}</button>
        </>
      }
    >
      <div className="set-row">
        <div className="grow">
          <div className="label">服务器地址</div>
          <div className="desc">LibreSpeed 后端地址,如 https://example.com/speedtest/backend</div>
        </div>
      </div>
      <input className="text-input mt8" style={{ width: "100%" }} value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://…" />
      <div className="set-row mt8">
        <div className="grow"><div className="label">显示名称(可选)</div></div>
      </div>
      <input className="text-input mt8" style={{ width: "100%" }} value={name} onChange={(e) => setName(e.target.value)} placeholder="留空自动使用主机名" />
      <div className="set-row mt12">
        <div>
          <div className="label">允许 HTTP(不加密)</div>
          <div className="desc">默认强制 HTTPS;内网自建节点可开启 HTTP</div>
        </div>
        <button className={`toggle ${allowHttp ? "on" : ""}`} onClick={() => setAllowHttp((v) => !v)} aria-label="允许 HTTP" />
      </div>
    </Modal>
  );
}
