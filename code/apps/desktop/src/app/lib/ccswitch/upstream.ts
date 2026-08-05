// d-token 上游规范化与比较：判断 cc-switch 当前上游与 d-token active upstream 是否一致。
// 关联计划：release-plan-2026-07/phase-02 §4 同步比较算法、UI 状态。
import type { UpstreamDescriptor } from "../agents/registry";
import type { CcSwitchSyncStatus } from "./types";

/** 规范化 URL：去除尾部斜杠，统一 localhost/127.0.0.1，解析端口，小写 host。 */
export function normalizeUrl(url: string | undefined): { host: string; port: number | undefined; path: string } {
  if (!url) return { host: "", port: undefined, path: "" };
  let text = url.trim();
  if (!/^https?:\/\//i.test(text) && !text.startsWith("http://")) {
    // 允许 host:port 形式
    text = `http://${text}`;
  }
  try {
    const parsed = new URL(text);
    let host = parsed.hostname.toLowerCase();
    if (host === "localhost") host = "127.0.0.1";
    const port = parsed.port ? Number(parsed.port) : parsed.protocol === "https:" ? 443 : 80;
    const path = parsed.pathname.replace(/\/+$/, "");
    return { host, port, path };
  } catch {
    return { host: text.toLowerCase(), port: undefined, path: "" };
  }
}

/** 规范化一个 UpstreamDescriptor 用于比较。 */
export function normalizeUpstream(up: UpstreamDescriptor | undefined) {
  if (!up) return null;
  const norm = normalizeUrl(up.baseUrl);
  return {
    protocol: up.protocol,
    host: norm.host,
    port: up.port ?? norm.port,
    path: norm.path,
    providerName: (up.providerName ?? "").toLowerCase(),
    profileName: (up.profileName ?? "").toLowerCase(),
  };
}

/** 比较两个上游是否一致（协议、host、port、path、provider/profile）。 */
export function upstreamsEqual(a: UpstreamDescriptor | undefined, b: UpstreamDescriptor | undefined): boolean {
  const na = normalizeUpstream(a);
  const nb = normalizeUpstream(b);
  if (!na || !nb) return false;
  return (
    na.protocol === nb.protocol &&
    na.host === nb.host &&
    na.port === nb.port &&
    na.path === nb.path &&
    (na.providerName === nb.providerName || na.providerName === "" || nb.providerName === "")
  );
}

/** 计算同步状态：synced / out-of-sync / needs-confirmation / unsupported / unknown。 */
export function computeSyncStatus(
  ccSwitchUpstream: UpstreamDescriptor | undefined,
  dtokenUpstream: UpstreamDescriptor | undefined,
): CcSwitchSyncStatus {
  if (!ccSwitchUpstream || !dtokenUpstream) return "unknown";
  if (ccSwitchUpstream.protocol === "unknown" || dtokenUpstream.protocol === "unknown") {
    return "needs-confirmation";
  }
  if (upstreamsEqual(ccSwitchUpstream, dtokenUpstream)) return "synced";
  // 字段不足时需确认
  if (!ccSwitchUpstream.baseUrl && !dtokenUpstream.baseUrl) return "needs-confirmation";
  return "out-of-sync";
}

/** UI 状态文案与主按钮（中文）。 */
export const SYNC_STATUS_UI: Record<CcSwitchSyncStatus, { label: string; action: string }> = {
  synced: { label: "d-token 已跟随当前 cc-switch 上游", action: "查看连接路径" },
  "out-of-sync": { label: "d-token 当前上游与 cc-switch 不一致", action: "同步上游" },
  "needs-confirmation": { label: "检测到 cc-switch 配置，但需要你确认上游", action: "确认上游" },
  unsupported: { label: "当前 cc-switch 配置格式暂不支持自动识别", action: "生成修复提示词" },
  unknown: { label: "还没有检测 cc-switch", action: "开始检测" },
};
