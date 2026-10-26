import { translateCopy, type Lang } from "./i18n";

export type UserOperation =
  | "connection"
  | "route"
  | "service"
  | "settings"
  | "learning"
  | "doctor"
  | "rollback"
  | "visual"
  | "codebaseMemory"
  | "update"
  | "generic";

export type UserFacingFailure = {
  title: string;
  message: string;
  details?: string;
};

/**
 * Converts native failures into release-facing copy. Native summaries remain
 * available to the Event Ledger, but never become the primary sentence in a
 * dialog, page state, or notification.
 */
export function presentOperationFailure(
  operation: UserOperation,
  reason: unknown,
  lang: Lang,
): UserFacingFailure {
  const label = translateCopy(`operation.${operation}`, lang);
  const failure = (messageKey: string, code: string): UserFacingFailure => ({
    title: translateCopy("operation.failureTitle", lang, { operation: label }),
    message: translateCopy(messageKey, lang),
    details: translateCopy("operation.statusCode", lang, { code }),
  });
  const normalized = normalizeReason(reason);

  if (normalized === "native_unavailable") {
    return {
      title: translateCopy("operation.failure.nativeUnavailable.title", lang),
      message: translateCopy("operation.failure.nativeUnavailable.message", lang),
      details: translateCopy("operation.statusCode", lang, { code: "NATIVE_UNAVAILABLE" }),
    };
  }
  if (normalized.includes("upstream_http_429") || /\b(?:http|status)[_: ]+429\b/.test(normalized) || normalized.includes("rate_limit") || normalized.includes("too many requests")) {
    return failure("operation.failure.rateLimited", "RATE_LIMITED");
  }
  if (normalized.includes("provider_rate_or_timeout")) {
    return failure("operation.failure.rateOrTimeout", "PROVIDER_BUSY");
  }
  if (normalized.includes("timeout") || normalized.includes("timed out")) {
    return failure("operation.failure.timeout", "REQUEST_TIMEOUT");
  }
  if (normalized.includes("source-config-changed") || normalized.includes("stale") || normalized.includes("changed after review") || normalized.includes("changed outside d-token")) {
    return failure("operation.failure.configChanged", "CONFIG_CHANGED");
  }
  if ((normalized.includes("port") || normalized.includes("address") || normalized.includes("bind"))
    && (normalized.includes("occupied") || normalized.includes("in use") || normalized.includes("bind"))) {
    return failure("operation.failure.portInUse", "PORT_IN_USE");
  }
  if (normalized.includes("credential") || normalized.includes("secret") || normalized.includes("plaintext")) {
    return failure("operation.failure.credential", "CREDENTIAL_REQUIRED");
  }
  if (normalized.includes("permission") || normalized.includes("denied") || normalized.includes("access")) {
    return failure("operation.failure.permission", "PERMISSION_REQUIRED");
  }
  if (normalized.includes("not found") || normalized.includes("missing")) {
    return failure("operation.failure.dataUnavailable", "DATA_UNAVAILABLE");
  }

  return failure("operation.failure.generic", "OPERATION_FAILED");
}

export function presentOperationSuccess(operation: UserOperation, lang: Lang): UserFacingFailure {
  return {
    title: translateCopy(`operation.${operation}`, lang),
    message: translateCopy(`operation.success.${operation}`, lang),
  };
}

function normalizeReason(reason: unknown) {
  if (reason instanceof Error) return reason.message.toLowerCase();
  if (typeof reason === "string") return reason.toLowerCase();
  if (reason && typeof reason === "object" && "summary" in reason) {
    const summary = (reason as { summary?: unknown }).summary;
    return typeof summary === "string" ? summary.toLowerCase() : "";
  }
  return "";
}
