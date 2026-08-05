export type RouteDraftApplyReview = {
  ok: boolean;
  source: string;
  mode: "route-apply-review";
  route_id: string;
  target: string;
  before_sha256: string;
  after_sha256: string;
  diff: string;
  risk_level: string;
  requires_explicit_confirmation: boolean;
  runtime_required: boolean;
  privacy?: string;
  error?: string;
};

export function isUsableRouteReview(value: unknown, expectedRouteId: string): value is RouteDraftApplyReview {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const review = value as Partial<RouteDraftApplyReview>;
  return review.ok === true
    && review.source === "dtoken-desktop-control-plane"
    && review.mode === "route-apply-review"
    && review.route_id === expectedRouteId
    && typeof review.target === "string"
    && review.target.trim().length > 0
    && typeof review.diff === "string"
    && review.diff.trim().length > 0
    && typeof review.risk_level === "string"
    && review.risk_level.trim().length > 0
    && isSha256(review.before_sha256)
    && isSha256(review.after_sha256)
    && review.requires_explicit_confirmation === true
    && review.runtime_required === false;
}

export function routeApplyRequest(review: RouteDraftApplyReview) {
  return {
    route_id: review.route_id,
    reviewed_before_sha256: review.before_sha256,
    reviewed_after_sha256: review.after_sha256,
  };
}

export function routeReviewFailureMessage(value: unknown, lang: "zh" | "en") {
  const error = value && typeof value === "object" && !Array.isArray(value)
    ? safeText((value as { error?: unknown }).error, "")
    : "";
  const normalized = error.toLowerCase();
  if (normalized.includes("not found")) {
    return lang === "zh" ? "无法查看路由变更：草稿不存在或已被移除。" : "Route changes are unavailable. The draft is missing or was removed.";
  }
  if (normalized.includes("plaintext-looking secret")) {
    return lang === "zh" ? "出于安全考虑，当前草稿不能继续处理。请先确认凭据保存方式。" : "The current draft cannot continue for safety reasons. Confirm how the credential is stored first.";
  }
  if (error) return lang === "zh" ? "无法准备路由变更。请刷新当前配置后重试。" : "Route changes could not be prepared. Refresh the current configuration and try again.";
  return lang === "zh"
    ? "无法准备路由变更：本机控制面未返回完整差异与 SHA-256 证据。"
    : "Route changes could not be prepared. Complete differences and SHA-256 evidence are unavailable.";
}

export function routeApplyFailureMessage(summary: unknown, lang: "zh" | "en") {
  const message = safeText(summary, "");
  const normalized = message.toLowerCase();
  if (normalized.includes("stale") || normalized.includes("review the current diff again")) {
    return lang === "zh"
      ? "应用已阻止：草稿或当前配置已发生变化，请重新查看当前差异。"
      : "Apply was blocked. The draft or active configuration changed. View the current differences again.";
  }
  if (normalized.includes("review the route diff")) {
    return lang === "zh" ? "应用已阻止：请先查看当前路由差异。" : "Apply was blocked: view the current route differences first.";
  }
  if (normalized.includes("could not be prepared")) {
    return lang === "zh" ? "路由草稿已变化或暂不可读，请重新生成变更预览。" : "The route draft changed or cannot be read. Generate a new change preview.";
  }
  if (normalized.includes("transaction failed") || normalized.includes("not committed")) {
    return lang === "zh" ? "配置事务失败且未提交；请重新查看当前变更后再试。" : "The configuration transaction failed and was not committed. View the current changes before retrying.";
  }
  if (normalized.includes("verification evidence")) {
    return lang === "zh" ? "写后验证证据缺失，未将本次操作标记为完成。" : "Post-write verification evidence is missing, so the operation was not marked complete.";
  }
  return lang === "zh"
    ? "路由应用未完成。请重新查看当前变更后再试。"
    : "Route apply did not finish. View the current changes before retrying.";
}

function isSha256(value: unknown) {
  return typeof value === "string" && /^[a-f0-9]{64}$/i.test(value);
}

function safeText(value: unknown, fallback: string) {
  return typeof value === "string" && value.length > 0 ? value : fallback;
}
