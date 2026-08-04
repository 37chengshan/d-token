// d-token 脱敏核心：统一处理密钥、token、cookie、认证头、完整 prompt/response 的脱敏。
// 设计目标：默认严格脱敏；debug 模式也不得导出真实密钥；只记录脱敏次数，不记录原文。
// 关联计划：release-plan-2026-07/phase-04 §5 日志模型与脱敏策略、phase-02 §5 强制规则 5。

export type RedactionLevel = "low" | "medium" | "high";

/** 禁止落盘的字段名（不区分大小写匹配）。 */
export const FORBIDDEN_FIELDS: readonly string[] = [
  "api_key",
  "apikey",
  "authorization",
  "cookie",
  "bearer",
  "token",
  "secret",
  "password",
  "passwd",
  "raw_prompt",
  "raw_response",
  "prompt",
  "response",
  "output",
  "private_key",
  "ssh_key",
  "credential",
  "client_secret",
];

/** 敏感内容正则指纹：用于值级别的脱敏检测。 */
const SECRET_PATTERNS: RegExp[] = [
  /sk-ant-[A-Za-z0-9_\-]{6,}/i, // Anthropic
  /sk-[A-Za-z0-9_\-]{6,}/i, // OpenAI
  /Bearer\s+[A-Za-z0-9_\-\.=]+/i,
  /AKIA[0-9A-Z]{12,}/, // AWS access key
  /-----BEGIN [A-Z ]*PRIVATE KEY-----/, // PEM
  /[A-Za-z0-9_\-]{32,}\.[A-Za-z0-9_\-]{16,}\.[A-Za-z0-9_\-]{16,}/, // JWT-ish
  /xox[baprs]-[A-Za-z0-9\-]{6,}/i, // Slack
  /gh[pousr]_[A-Za-z0-9]{16,}/i, // GitHub
];

/** 脱敏后的值。保留类型与末 4 位以便识别，但永不暴露完整原文。 */
export type RedactedValue =
  | string
  | number
  | boolean
  | null
  | RedactedValue[]
  | { [key: string]: RedactedValue };

export type RedactionResult = {
  value: RedactedValue;
  redactionCount: number;
};

/** 判断字段名是否属于禁止落盘字段。 */
export function isForbiddenField(name: string): boolean {
  const lower = name.toLowerCase();
  return FORBIDDEN_FIELDS.some((field) => lower === field || lower.includes(field));
}

/** 判断值是否匹配敏感指纹。 */
export function looksLikeSecret(value: unknown): boolean {
  if (typeof value !== "string") return false;
  return SECRET_PATTERNS.some((pattern) => pattern.test(value));
}

/** 对单个值脱敏：保留类型与末 4 位（仅在 level >= medium 且长度足够时），否则整体替换。 */
export function redactValue(value: unknown, level: RedactionLevel = "high"): { redacted: string; count: number } {
  if (value == null) return { redacted: String(value), count: 0 };
  const text = typeof value === "string" ? value : String(value);
  if (text.length === 0) return { redacted: "", count: 0 };
  // high 模式：永远不保留任何原文片段
  if (level === "high") {
    return { redacted: "[REDACTED]", count: 1 };
  }
  // medium：保留末 4 位用于识别
  if (level === "medium" && text.length > 8) {
    return { redacted: `[REDACTED …${text.slice(-4)}]`, count: 1 };
  }
  // low：保留末 4 位
  if (level === "low" && text.length > 8) {
    return { redacted: `[REDACTED …${text.slice(-4)}]`, count: 1 };
  }
  return { redacted: "[REDACTED]", count: 1 };
}

/** 递归脱敏一个对象/数组：字段名命中禁止表或值命中指纹时脱敏。 */
export function redactObject<T>(input: T, level: RedactionLevel = "high"): RedactionResult {
  let count = 0;
  const walk = (value: unknown): RedactedValue => {
    if (Array.isArray(value)) {
      return value.map((item) => walk(item));
    }
    if (value && typeof value === "object") {
      const out: Record<string, RedactedValue> = {};
      for (const [key, raw] of Object.entries(value as Record<string, unknown>)) {
        if (isForbiddenField(key) || looksLikeSecret(raw)) {
          const { redacted, count: c } = redactValue(raw, level);
          out[key] = redacted;
          count += c;
        } else if (raw && typeof raw === "object") {
          out[key] = walk(raw);
        } else {
          out[key] = (raw as RedactedValue) ?? null;
        }
      }
      return out;
    }
    if (looksLikeSecret(value)) {
      const { redacted, count: c } = redactValue(value, level);
      count += c;
      return redacted;
    }
    return (value as RedactedValue) ?? null;
  };
  const redacted = walk(input);
  return { value: redacted, redactionCount: count };
}

/** 脱敏一个用于展示/导出的字符串（如 base URL 中的凭据）。 */
export function redactString(text: string, level: RedactionLevel = "high"): string {
  if (!text) return text;
  let result = text;
  let didRedact = false;
  for (const pattern of SECRET_PATTERNS) {
    if (pattern.test(result)) {
      result = result.replace(pattern, "[REDACTED]");
      didRedact = true;
    }
  }
  // 脱敏 URL 内嵌凭据 user:pass@host
  result = result.replace(/\/\/[^:/\s]+:[^@/\s]+@/g, "//[REDACTED]@");
  void didRedact;
  void level;
  return result;
}

/** 校验导出内容是否含有未脱敏的敏感字段；返回违规字段名列表。 */
export function auditExportForSecrets(payload: unknown): string[] {
  const violations: string[] = [];
  const walk = (value: unknown, path: string) => {
    if (Array.isArray(value)) {
      value.forEach((item, i) => walk(item, `${path}[${i}]`));
      return;
    }
    if (value && typeof value === "object") {
      for (const [key, raw] of Object.entries(value as Record<string, unknown>)) {
        const nextPath = path ? `${path}.${key}` : key;
        if (isForbiddenField(key)) {
          if (typeof raw === "string" && raw !== "[REDACTED]" && !raw.startsWith("[REDACTED")) {
            violations.push(nextPath);
          }
        } else if (looksLikeSecret(raw)) {
          violations.push(nextPath);
        }
        if (raw && typeof raw === "object") walk(raw, nextPath);
      }
    }
  };
  walk(payload, "");
  return violations;
}
