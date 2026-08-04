import { useEffect, useRef, useState } from "react";
import { Bell, CheckCheck, ChevronRight, X } from "lucide-react";
import { useI18n } from "../../lib/i18n";
import { useNotifications, type AppNotification } from "../../lib/notifications";
import { Popover, PopoverContent, PopoverTrigger } from "../ui/popover";
import { cn } from "../ui/utils";

export function NotificationCenter() {
  const { lang } = useI18n();
  const { notifications, unreadCount, markRead, markAllRead, dismiss } = useNotifications();
  const [open, setOpen] = useState(false);
  const [expanded, setExpanded] = useState<string | null>(null);

  return (
    <Popover open={open} onOpenChange={setOpen}>
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
        <PopoverContent align="end" sideOffset={6} className="notification-popover p-0" aria-label={lang === "zh" ? "通知中心" : "Notification center"}>
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
  return (
    <article className={cn("notification-row", !item.read && "is-unread")} data-level={item.level}>
      <button type="button" onClick={onToggle} className="min-w-0 flex-1 text-left">
        <div className="flex items-center gap-2">
          <span className="notification-level-dot" aria-hidden="true" />
          <span className="truncate text-sm font-medium">{item.title}</span>
          {item.count > 1 && <span className="text-[10px] text-muted-foreground">×{item.count}</span>}
          <ChevronRight className={cn("ml-auto size-3.5 text-muted-foreground transition-transform", expanded && "rotate-90")} />
        </div>
        <p className="mt-1 line-clamp-2 text-xs leading-5 text-muted-foreground">{item.message}</p>
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
  const { notifications, markRead, dismiss } = useNotifications();
  const consumedSignaturesRef = useRef(new Set(notifications.map((item) => `${item.id}:${item.count}`)));
  const [bannerId, setBannerId] = useState<string | null>(null);
  const newest = notifications[0];
  const important = bannerId
    ? notifications.find((item) => item.id === bannerId && !item.read)
    : undefined;

  // The shell banner is a latest-state surface, not an unread queue. Older
  // history remains in NotificationCenter and must never replay after the
  // current banner is read, dismissed, or expires.
  useEffect(() => {
    if (!newest || newest.read) return;
    const signature = `${newest.id}:${newest.count}`;
    if (consumedSignaturesRef.current.has(signature)) return;
    consumedSignaturesRef.current.add(signature);
    setBannerId(newest.id);
  }, [newest]);

  // Informational and success receipts are useful at the top of the shell,
  // but should not pin the banner forever when they are intentionally
  // transient. Persistent receipts/errors remain unread in the notification
  // center after the banner itself recedes.
  useEffect(() => {
    if (!important) return;
    const timer = window.setTimeout(() => {
      setBannerId(null);
      if (!important.persistent) markRead(important.id);
    }, important.level === "error" || important.level === "action-required" ? 8000 : 5000);
    return () => window.clearTimeout(timer);
  }, [important, markRead]);

  if (!important) return null;
  return (
    <div className="notification-banner" data-level={important.level} role="status" aria-live="polite">
      <span className="notification-level-dot" aria-hidden="true" />
      <strong>{important.title}</strong>
      <span className="min-w-0 flex-1 truncate text-muted-foreground" title={important.details ? `${important.message} · ${important.details}` : important.message}>
        {important.message}
        {important.details ? <span className="ml-1 text-[11px] opacity-75">· {important.details}</span> : null}
      </span>
      <button type="button" onClick={() => { setBannerId(null); markRead(important.id); }} className="text-xs font-medium">{important.level === "action-required" ? (lang === "zh" ? "查看" : "View") : (lang === "zh" ? "知道了" : "Dismiss")}</button>
      <button type="button" onClick={() => { setBannerId(null); dismiss(important.id); }} aria-label={lang === "zh" ? "移除通知" : "Dismiss notification"}><X className="size-3.5" /></button>
    </div>
  );
}

export function NotificationLiveRegion() {
  const { notifications } = useNotifications();
  const latest = notifications[0];
  return <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">{latest ? `${latest.title}: ${latest.message}` : ""}</div>;
}
