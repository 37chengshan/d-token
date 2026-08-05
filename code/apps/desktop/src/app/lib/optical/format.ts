import { translateCopy, type Lang } from "../i18n";

export function formatOpticalMode(mode: string | undefined, lang: Lang): string {
  if (!mode) return "—";
  const key = mode.toLowerCase();
  const copyKey = `optical.mode.${key}`;
  const formatted = translateCopy(copyKey, lang);
  return formatted === copyKey ? mode : formatted;
}

export function formatOpticalStatus(status: string | undefined, lang: Lang): string {
  if (!status) return "—";
  const key = status.toLowerCase();
  const copyKey = `optical.status.${key}`;
  const formatted = translateCopy(copyKey, lang);
  return formatted === copyKey ? status : formatted;
}

export function formatOpticalStage(stage: string | undefined, lang: Lang): string {
  if (!stage) return "—";
  const key = stage.toLowerCase();
  const copyKey = `optical.stage.${key}`;
  const formatted = translateCopy(copyKey, lang);
  return formatted === copyKey ? stage : formatted;
}

export function formatTimestampMs(value: number | undefined, lang: Lang): string {
  if (!value || !Number.isFinite(value)) return "—";
  try {
    return new Date(value).toLocaleString(lang === "zh" ? "zh-CN" : "en-US");
  } catch {
    return "—";
  }
}

export function displayCount(value: number | undefined | null, available: boolean): string {
  if (!available) return "—";
  if (value === undefined || value === null || !Number.isFinite(value)) return "—";
  return value.toLocaleString("en-US");
}
