import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Bell, CheckCheck, ChevronRight, CircleCheck, CircleAlert, Info, X } from "lucide-react";
import { useI18n } from "../../lib/i18n";
import { useNotifications, type AppNotification, type NotificationToast } from "../../lib/notifications";
import { Popover, PopoverContent, PopoverTrigger } from "../ui/popover";
import { cn } from "../ui/utils";

export function NotificationCenter() {
  const { lang } = useI18n();
  const { notifications, unreadCount, markRead, markAllRead, dismiss, centerOpen, setCenterOpen, selectedNotificationId } = useNotifications();
  const [expanded, setExpanded] = useState<string | null>(null);

  useEffect(() => {
    if (centerOpen) setExpanded(selectedNotificationId);
  }, [centerOpen, selectedNotificationId]);

  return (
    <Popover open={centerOpen} onOpenChange={setCenterOpen}>
      <div className="notification-center shell-window-control">
        <PopoverTrigger asChild>
          <button
            type="button"
            aria-label={lang === "zh" ? "通知" : "Notifications"}
            className="relative flex size-8 items-center justify-center rounded-lg text-muted-foreground hover:bg-[var(--chip-hover-bg)] hover:text-foreground"
          >
            <Bell className="size-4" />
            {unreadCount > 0 && <span className="notification-unread-dot" aria-label={`${unreadCount}`} />}
          </button>
        </PopoverTrigger>
        <PopoverContent align="end" sideOffset={6} className="notification-popover p-0" aria-label={lang === "zh" ? "通知中心" : "Notification center"}
          onOpenAutoFocus={(event) => { if (selectedNotificationId) event.preventDefault(); }}>
          <div className="flex items-center justify-between border-b border-border px-3 py-2.5">
            <strong className="text-sm">{lang === "zh" ? "通知" : "Notifications"}</strong>
            <button type="button" onClick={markAllRead} className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground">
              <CheckCheck className="size-3.5" />{lang === "zh" ? "全部已读" : "Mark all read"}
            </button>
          </div>
          <div className="max-h-[360px] overflow-y-auto">
            {notifications.length === 0 ? (
              <div className="px-4 py-8 text-center text-sm text-muted-foreground">{lang === "zh" ? "暂无通知" : "No notifications"}</div>
            ) : notifications.map((item) => (
              <NotificationRow
                key={item.id}
                item={item}
                expanded={expanded === item.id}
                onToggle={() => {
                  markRead(item.id);
                  setExpanded((current) => current === item.id ? null : item.id);
                }}
                onDismiss={() => dismiss(item.id)}
                lang={lang}
              />
            ))}
          </div>
        </PopoverContent>
      </div>
    </Popover>
  );
}

function NotificationRow({ item, expanded, onToggle, onDismiss, lang }: { item: AppNotification; expanded: boolean; onToggle: () => void; onDismiss: () => void; lang: "zh" | "en" }) {
  const row = useRef<HTMLElement>(null);
  useEffect(() => {
    if (expanded) {
      row.current?.scrollIntoView?.({ block: "nearest" });
      row.current?.querySelector("button")?.focus({ preventScroll: true });
    }
  }, [expanded]);
  return (
    <article ref={row} className={cn("notification-row", !item.read && "is-unread")} data-level={item.level}>
      <button type="button" onClick={onToggle} aria-expanded={expanded} className="min-w-0 flex-1 text-left">
        <div className="flex items-center gap-2">
          <span className="notification-level-dot" aria-hidden="true" />
          <span className="truncate text-sm font-medium">{item.title}</span>
          {item.count > 1 && <span className="text-[10px] text-muted-foreground">×{item.count}</span>}
          <ChevronRight className={cn("ml-auto size-3.5 text-muted-foreground transition-transform", expanded && "rotate-90")} />
        </div>
        <p className={cn("mt-1 break-words text-xs leading-5 text-muted-foreground", !expanded && "line-clamp-2")}>{item.message}</p>
        <time className="mt-1 block text-[10px] text-muted-foreground">{new Date(item.timestamp).toLocaleTimeString()}</time>
        {expanded && item.details && <p className="mt-2 rounded-md bg-muted/55 p-2 text-xs leading-5 text-muted-foreground">{item.details}</p>}
        {expanded && item.receipt && (
          <dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-2 rounded-md bg-muted/55 p-2 text-[11px] text-muted-foreground">
            <dt>{lang === "zh" ? "请求" : "Request"}</dt><dd className="truncate font-mono">{item.receipt.requestId}</dd>
            {item.receipt.rollbackId && <><dt>{lang === "zh" ? "回滚" : "Rollback"}</dt><dd className="truncate font-mono">{item.receipt.rollbackId}</dd></>}
            {item.receipt.targetId && <><dt>{lang === "zh" ? "目标" : "Target"}</dt><dd className="truncate font-mono">{item.receipt.targetId}</dd></>}
          </dl>
        )}
      </button>
      <button type="button" onClick={onDismiss} aria-label={lang === "zh" ? "移除通知" : "Dismiss notification"} className="mt-0.5 text-muted-foreground hover:text-foreground"><X className="size-3.5" /></button>
    </article>
  );
}

export function NotificationBanner() {
  const { lang } = useI18n();
  const { toasts } = useNotifications();
  if (typeof document === "undefined") return null;
  return createPortal(
    <ol className="notification-toasts" aria-label={lang === "zh" ? "操作提示" : "Operation notifications"}>
      {toasts.map((toast) => <Toast key={toast.key} toast={toast} lang={lang} />)}
    </ol>,
    document.body,
  );
}

export function NotificationLiveRegion() {
  const { toasts } = useNotifications();
  const latestToast = toasts[toasts.length - 1];
  return (
    <div className="sr-only" aria-live="polite" aria-atomic="true">
      {latestToast ? `${latestToast.notification.title}: ${latestToast.notification.message}` : ""}
    </div>
  );
}

function Toast({ toast, lang }: { toast: NotificationToast; lang: "zh" | "en" }) {
  const { dismissToast, openNotification } = useNotifications();
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const [closing, setClosing] = useState(false);
  const remaining = useRef(2000);
  const paused = hovered || focused;
  const item = toast.notification;

  // Resume the unused part of the two seconds, rather than restarting the
  // timer on every focus move or unrelated notification update.
  useEffect(() => {
    if (paused || closing) return;
    const started = Date.now();
    const timer = window.setTimeout(() => setClosing(true), remaining.current);
    return () => {
      window.clearTimeout(timer);
      remaining.current = Math.max(0, remaining.current - (Date.now() - started));
    };
  }, [paused, closing]);
  useEffect(() => {
    if (!closing) return;
    const timer = window.setTimeout(() => dismissToast(toast.key), 160);
    return () => window.clearTimeout(timer);
  }, [closing, dismissToast, toast.key]);

  const Icon = item.level === "success" ? CircleCheck : item.level === "info" ? Info : CircleAlert;
  return (
    <li className="notification-toast" data-level={item.level} data-state={closing ? "closing" : "open"}
      onMouseEnter={() => setHovered(true)} onMouseLeave={() => setHovered(false)}
      onFocusCapture={() => setFocused(true)}
      onBlurCapture={(event) => { if (!event.currentTarget.contains(event.relatedTarget)) setFocused(false); }}>
      <Icon className="notification-toast-icon" aria-hidden="true" />
      <div className="notification-toast-copy" role="status" aria-live="polite" aria-atomic="true">
        <strong>{item.title}</strong>
        {item.message && item.message !== item.title && <p>{item.message}</p>}
      </div>
      <button type="button" className="notification-toast-view" onClick={() => openNotification(item.id)}>{lang === "zh" ? "查看" : "View"}</button>
      <button type="button" className="notification-toast-close" onClick={() => dismissToast(toast.key)} aria-label={lang === "zh" ? "关闭提示" : "Close toast"}><X className="size-3.5" /></button>
    </li>
  );
}
