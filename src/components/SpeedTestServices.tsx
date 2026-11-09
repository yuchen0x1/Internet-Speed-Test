import { Icon } from "./Icon";

const GROUPS = [
  {
    title: "国内测速站",
    hint: "打开官网独立测速 · 省份/运营商节点可能有地域限制",
    services: [
      { id: "speedtest_cn", name: "测速网 SpeedTest.cn", region: "全国 · 多运营商", tone: "cyan", icon: "gauge" as const },
      { id: "telecom_gd", name: "广东电信宽带测速", region: "中国电信 · 广东入口", tone: "red", icon: "ethernet" as const },
      { id: "unicom", name: "中国联通宽带测速", region: "中国联通 · 营业厅入口", tone: "red", icon: "wifi" as const },
      { id: "mobile_bj", name: "中国移动宽带测速", region: "中国移动 · 北京入口", tone: "green", icon: "wifi" as const },
    ],
  },
  {
    title: "国际测速站",
    hint: "官方公开测速服务 · 由官网在独立浏览器标签中运行",
    services: [
      { id: "ookla", name: "Ookla Speedtest", region: "全球服务器网络", tone: "cyan", icon: "zap" as const },
      { id: "fast", name: "Netflix FAST.com", region: "Netflix Open Connect", tone: "red", icon: "play" as const },
      { id: "cloudflare", name: "Cloudflare Speed Test", region: "全球边缘网络", tone: "orange", icon: "globe" as const },
    ],
  },
] as const;

export function SpeedTestServices(props: {
  opening: string | null;
  onOpen: (id: string, name: string) => void;
}) {
  return (
    <section className="card pad speed-services">
      <div className="row speed-services-head">
        <div>
          <div className="chart-title"><Icon name="globe" size={16} /> 国内外知名测速服务</div>
          <div className="faint speed-services-note">官网测速会在系统浏览器单独运行，不会混入 NetVision 本机历史结果。</div>
        </div>
        <span className="chip">官网入口</span>
      </div>
      <div className="speed-service-groups">
        {GROUPS.map((group) => (
          <div className="speed-service-group" key={group.title}>
            <div className="speed-service-group-head">
              <b>{group.title}</b><span>{group.hint}</span>
            </div>
            <div className="speed-service-list">
              {group.services.map((service) => (
                <button
                  className={`speed-service-card ${service.tone}`}
                  key={service.id}
                  onClick={() => props.onOpen(service.id, service.name)}
                  disabled={props.opening != null}
                  title={`在系统浏览器打开 ${service.name}`}
                >
                  <span className="speed-service-icon"><Icon name={service.icon} size={17} /></span>
                  <span className="speed-service-copy">
                    <b>{service.name}</b>
                    <small>{service.region}</small>
                  </span>
                  <span className="speed-service-open">{props.opening === service.id ? "打开中…" : "打开"}<Icon name="chevron-right" size={13} /></span>
                </button>
              ))}
            </div>
          </div>
        ))}
      </div>
      <div className="speed-service-foot"><span className="status-dot ok" /> 官网入口与应用内测速分离；应用内可选 LibreSpeed / Cloudflare / M-Lab NDT7 协议节点</div>
    </section>
  );
}
