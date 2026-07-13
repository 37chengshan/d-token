import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { redactString } from "./security/redaction";

export type NotificationLevel = "info" | "success" | "warning" | "error" | "action-required";

export type AppNotification = {
  id: string;
  dedupeKey: string;
  level: NotificationLevel;
  title: string;
  message: string;
  details?: string;
  timestamp: number;
  read: boolean;
  persistent: boolean;
  count: number;
  receipt?: NotificationReceipt;
};

export type NotificationReceipt = {
  requestId: string;
  transactionId?: string;
  rollbackId?: string;
  targetId?: string;
};

export type NotificationInput = Omit<AppNotification, "id" | "timestamp" | "read" | "count" | "persistent"> & {
  id?: string;
  timestamp?: number;
  persistent?: boolean;
};

type NotificationContextValue = {
  notifications: AppNotification[];
  unreadCount: number;
  addNotification: (input: NotificationInput) => string;
  markRead: (id: string) => void;
  markAllRead: () => void;
  dismiss: (id: string) => void;
};

const STORAGE_KEY = "d-token.notifications.v2";
const LEGACY_STORAGE_KEY = "d-token.notifications.v1";
const STORAGE_SCHEMA_VERSION = 2;
const MAX_NOTIFICATIONS = 50;

const fallback: NotificationContextValue = {
  notifications: [],
  unreadCount: 0,
  addNotification: () => "",
  markRead: () => {},
  markAllRead: () => {},
  dismiss: () => {},
};

const NotificationContext = createContext<NotificationContextValue>(fallback);

export function useNotifications() {
  return useContext(NotificationContext);
}

export function createOperationRequestId(scope: string) {
  const suffix = typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
    ? crypto.randomUUID()
    : `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
  return `${scope}:${suffix}`;
}

export function NotificationProvider({ children }: { children: ReactNode }) {
  const [notifications, setNotifications] = useState<AppNotification[]>(readPersistedNotifications);

  useEffect(() => {
    persistNotifications(notifications);
  }, [notifications]);

  const addNotification = useCallback((input: NotificationInput) => {
    const timestamp = input.timestamp ?? Date.now();
    const id = input.id ?? `${timestamp}-${Math.random().toString(36).slice(2, 8)}`;
    const persistent = input.persistent ?? isPersistentLevel(input.level);
    const safe = sanitizeInput(input);
    setNotifications((current) => {
      const existing = current.find((item) => item.dedupeKey === input.dedupeKey);
      const next: AppNotification = existing
        ? { ...existing, ...safe, timestamp, read: false, persistent, count: existing.count + 1 }
        : { ...safe, id, timestamp, read: false, persistent, count: 1 };
      return [next, ...current.filter((item) => item.dedupeKey !== input.dedupeKey)].slice(0, MAX_NOTIFICATIONS);
    });
    return id;
  }, []);

  const markRead = useCallback((id: string) => {
    setNotifications((current) => current.map((item) => item.id === id ? { ...item, read: true } : item));
  }, []);
  const markAllRead = useCallback(() => {
    setNotifications((current) => current.map((item) => ({ ...item, read: true })));
  }, []);
  const dismiss = useCallback((id: string) => {
    setNotifications((current) => current.filter((item) => item.id !== id));
  }, []);

  const value = useMemo<NotificationContextValue>(() => ({
    notifications,
    unreadCount: notifications.filter((item) => !item.read).length,
    addNotification,
    markRead,
    markAllRead,
    dismiss,
  }), [addNotification, dismiss, markAllRead, markRead, notifications]);

  return <NotificationContext.Provider value={value}>{children}</NotificationContext.Provider>;
}

function sanitizeInput(input: NotificationInput): Pick<AppNotification, "dedupeKey" | "level" | "title" | "message" | "details" | "receipt"> {
  return {
    dedupeKey: redactString(input.dedupeKey),
    level: input.level,
    title: redactString(input.title),
    message: redactString(input.message),
    ...(input.details ? { details: redactString(input.details) } : {}),
    ...(input.receipt ? { receipt: sanitizeReceipt(input.receipt) } : {}),
  };
}

function sanitizeReceipt(receipt: NotificationReceipt): NotificationReceipt {
  return {
    requestId: redactString(receipt.requestId),
    ...(receipt.transactionId ? { transactionId: redactString(receipt.transactionId) } : {}),
    ...(receipt.rollbackId ? { rollbackId: redactString(receipt.rollbackId) } : {}),
    ...(receipt.targetId ? { targetId: redactString(receipt.targetId) } : {}),
  };
}

function isPersistentLevel(level: NotificationLevel) {
  return level === "warning" || level === "error" || level === "action-required";
}

function readPersistedNotifications(): AppNotification[] {
  if (typeof window === "undefined") return [];
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    if (stored === null) {
      // v1 was used only by pre-release development builds and contains no
      // schema or trustworthy scope binding. Do not carry those persistent
      // messages into the release candidate; later writes use the v2 envelope.
      window.localStorage.removeItem(LEGACY_STORAGE_KEY);
      return [];
    }
    const parsed = JSON.parse(stored) as { schemaVersion?: unknown; notifications?: unknown };
    if (parsed?.schemaVersion !== STORAGE_SCHEMA_VERSION || !Array.isArray(parsed.notifications)) return [];
    return parsed.notifications.filter(isStoredNotification).slice(0, MAX_NOTIFICATIONS);
  } catch {
    return [];
  }
}

function persistNotifications(notifications: AppNotification[]) {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify({
      schemaVersion: STORAGE_SCHEMA_VERSION,
      notifications: notifications.filter((item) => item.persistent),
    }));
    window.localStorage.removeItem(LEGACY_STORAGE_KEY);
  } catch {
    // Notification persistence must never block the control plane.
  }
}

function isStoredNotification(value: unknown): value is AppNotification {
  if (!value || typeof value !== "object") return false;
  const item = value as Partial<AppNotification>;
  return typeof item.id === "string"
    && typeof item.dedupeKey === "string"
    && typeof item.title === "string"
    && typeof item.message === "string"
    && typeof item.timestamp === "number"
    && typeof item.read === "boolean"
    && item.persistent === true
    && typeof item.count === "number"
    && ["info", "success", "warning", "error", "action-required"].includes(item.level ?? "");
}
