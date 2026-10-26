import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
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
  toasts: NotificationToast[];
  centerOpen: boolean;
  selectedNotificationId: string | null;
  unreadCount: number;
  addNotification: (input: NotificationInput) => string;
  markRead: (id: string) => void;
  markAllRead: () => void;
  dismiss: (id: string) => void;
  dismissToast: (key: number) => void;
  openNotification: (id: string) => void;
  setCenterOpen: (open: boolean) => void;
};

export type NotificationToast = { key: number; notification: AppNotification };

const STORAGE_KEY = "d-token.notifications.v2";
const LEGACY_STORAGE_KEY = "d-token.notifications.v1";
const STORAGE_SCHEMA_VERSION = 2;
const MAX_NOTIFICATIONS = 50;
const MAX_TOASTS = 3;

const fallback: NotificationContextValue = {
  notifications: [],
  toasts: [],
  centerOpen: false,
  selectedNotificationId: null,
  unreadCount: 0,
  addNotification: () => "",
  markRead: () => {},
  markAllRead: () => {},
  dismiss: () => {},
  dismissToast: () => {},
  openNotification: () => {},
  setCenterOpen: () => {},
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
  const currentNotifications = useRef(notifications);
  // Ephemeral delivery is separate from history. Mounting the shell or loading
  // persisted records must not replay old failures as new operation feedback.
  const [toasts, setToasts] = useState<NotificationToast[]>([]);
  const toastSequence = useRef(0);
  const [centerOpen, setCenterOpen] = useState(false);
  const [selectedNotificationId, setSelectedNotificationId] = useState<string | null>(null);

  const updateNotifications = useCallback((next: AppNotification[]) => {
    currentNotifications.current = next;
    setNotifications(next);
  }, []);

  useEffect(() => {
    persistNotifications(notifications);
  }, [notifications]);

  const addNotification = useCallback((input: NotificationInput) => {
    const timestamp = input.timestamp ?? Date.now();
    const persistent = input.persistent ?? isPersistentLevel(input.level);
    const safe = sanitizeInput(input);
    const current = currentNotifications.current;
    const existing = current.find((item) => item.dedupeKey === safe.dedupeKey);
    const id = existing?.id ?? input.id ?? createOperationRequestId("notification");
    // Replace optional details/receipt as well: a successful retry must not
    // inherit the failed attempt's explanation or recovery reference.
    const next: AppNotification = { ...safe, id, timestamp, read: false, persistent, count: (existing?.count ?? 0) + 1 };
    updateNotifications([next, ...current.filter((item) => item.id !== id)].slice(0, MAX_NOTIFICATIONS));
    const toast = { key: ++toastSequence.current, notification: next };
    setToasts((current) => [...current.filter((item) => item.notification.id !== id), toast].slice(-MAX_TOASTS));
    return id;
  }, [updateNotifications]);

  const markRead = useCallback((id: string) => {
    updateNotifications(currentNotifications.current.map((item) => item.id === id ? { ...item, read: true } : item));
  }, [updateNotifications]);
  const markAllRead = useCallback(() => {
    updateNotifications(currentNotifications.current.map((item) => ({ ...item, read: true })));
  }, [updateNotifications]);
  const dismiss = useCallback((id: string) => {
    updateNotifications(currentNotifications.current.filter((item) => item.id !== id));
    setToasts((current) => current.filter((item) => item.notification.id !== id));
  }, [updateNotifications]);
  const dismissToast = useCallback((key: number) => {
    setToasts((current) => current.filter((item) => item.key !== key));
  }, []);
  const openNotification = useCallback((id: string) => {
    setSelectedNotificationId(id);
    setCenterOpen(true);
    markRead(id);
    setToasts((current) => current.filter((item) => item.notification.id !== id));
  }, [markRead]);

  const value = useMemo<NotificationContextValue>(() => ({
    notifications,
    toasts,
    centerOpen,
    selectedNotificationId,
    unreadCount: notifications.filter((item) => !item.read).length,
    addNotification,
    markRead,
    markAllRead,
    dismiss,
    dismissToast,
    openNotification,
    setCenterOpen,
  }), [addNotification, dismiss, dismissToast, markAllRead, markRead, notifications, toasts, centerOpen, selectedNotificationId, openNotification]);

  return <NotificationContext.Provider value={value}>{children}</NotificationContext.Provider>;
}

function sanitizeInput(input: NotificationInput): Pick<AppNotification, "dedupeKey" | "level" | "title" | "message" | "details" | "receipt"> {
  return {
    dedupeKey: redactAndLimit(input.dedupeKey, 160),
    level: input.level,
    title: redactAndLimit(input.title, 100),
    message: redactAndLimit(input.message, 280),
    ...(input.details ? { details: redactAndLimit(input.details, 400) } : {}),
    ...(input.receipt ? { receipt: sanitizeReceipt(input.receipt) } : {}),
  };
}

function redactAndLimit(value: string, maxLength: number): string {
  const safe = redactString(value).replace(/\s+/g, " ").trim();
  return safe.length <= maxLength ? safe : `${safe.slice(0, Math.max(0, maxLength - 1))}…`;
}

function sanitizeReceipt(receipt: NotificationReceipt): NotificationReceipt {
  return {
    requestId: redactAndLimit(receipt.requestId, 160),
    ...(receipt.transactionId ? { transactionId: redactAndLimit(receipt.transactionId, 160) } : {}),
    ...(receipt.rollbackId ? { rollbackId: redactAndLimit(receipt.rollbackId, 160) } : {}),
    ...(receipt.targetId ? { targetId: redactAndLimit(receipt.targetId, 160) } : {}),
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
    return parsed.notifications.filter(isStoredNotification).slice(0, MAX_NOTIFICATIONS)
      .map((item) => ({ ...item, ...sanitizeInput(item) }));
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
    && typeof item.timestamp === "number" && Number.isFinite(item.timestamp)
    && typeof item.read === "boolean"
    && item.persistent === true
    && typeof item.count === "number" && Number.isInteger(item.count) && item.count > 0
    && (item.details === undefined || typeof item.details === "string")
    && (item.receipt === undefined || Boolean(item.receipt)
      && typeof item.receipt.requestId === "string"
      && [item.receipt.transactionId, item.receipt.rollbackId, item.receipt.targetId].every((value) => value === undefined || typeof value === "string"))
    && ["info", "success", "warning", "error", "action-required"].includes(item.level ?? "");
}
