export type RefreshSchedulerOptions = {
  fallbackDelayMs?: () => number | null;
};

export type ScheduledRefresh<T> = {
  /** Explicit user refreshes run immediately, even while the window is hidden. */
  refresh: () => Promise<T>;
  /** Event refreshes wait for visibility when the window is hidden. */
  invalidate: () => Promise<T | undefined>;
  release: () => void;
};

type RefreshOwner<T = unknown> = {
  task: () => Promise<T>;
  fallbackDelayMs?: () => number | null;
};

type RefreshEntry = {
  key: string;
  owners: Map<symbol, RefreshOwner>;
  generation: number;
  timer: number | undefined;
};

/**
 * Owns one fallback timer and one in-flight task per key for the whole App.
 * Registrations are interests: the timer exists only while at least one owner
 * is interested, and low-priority work pauses while the document is hidden.
 */
export class KeyedRefreshScheduler {
  private readonly entries = new Map<string, RefreshEntry>();
  private readonly inFlight = new Map<string, Promise<unknown>>();
  private visibilityState: DocumentVisibilityState;

  constructor(
    private readonly documentTarget: Document = document,
    private readonly windowTarget: Window = window,
  ) {
    this.visibilityState = documentTarget.visibilityState;
    documentTarget.addEventListener("visibilitychange", this.handleVisibilityChange);
  }

  register<T>(key: string, task: () => Promise<T>, options: RefreshSchedulerOptions = {}): ScheduledRefresh<T> {
    const ownerId = Symbol(key);
    let entry = this.entries.get(key);
    if (!entry) {
      entry = { key, owners: new Map(), generation: 0, timer: undefined };
      this.entries.set(key, entry);
    }
    entry.owners.set(ownerId, { task, fallbackDelayMs: options.fallbackDelayMs });

    if (entry.owners.size === 1) {
      void this.request(entry, ownerId, true).catch(() => undefined);
    }

    let released = false;
    return {
      refresh: () => this.request(entry!, ownerId, false) as Promise<T>,
      invalidate: () => this.request(entry!, ownerId, true) as Promise<T | undefined>,
      release: () => {
        if (released) return;
        released = true;
        this.release(entry!, ownerId);
      },
    };
  }

  dispose(): void {
    this.documentTarget.removeEventListener("visibilitychange", this.handleVisibilityChange);
    for (const entry of this.entries.values()) this.stopEntry(entry);
    this.entries.clear();
  }

  private readonly handleVisibilityChange = () => {
    const nextVisibility = this.documentTarget.visibilityState;
    if (nextVisibility === this.visibilityState) return;
    this.visibilityState = nextVisibility;

    if (nextVisibility === "hidden") {
      for (const entry of this.entries.values()) this.stopEntry(entry);
      return;
    }

    for (const entry of this.entries.values()) {
      const ownerId = entry.owners.keys().next().value as symbol | undefined;
      if (ownerId) void this.request(entry, ownerId, true).catch(() => undefined);
    }
  };

  private request<T>(entry: RefreshEntry, ownerId: symbol, respectVisibility: boolean): Promise<T | undefined> {
    const owner = entry.owners.get(ownerId) as RefreshOwner<T> | undefined;
    if (!owner || this.entries.get(entry.key) !== entry) return Promise.resolve(undefined);

    entry.generation += 1;
    const expectedGeneration = entry.generation;
    this.clearTimer(entry);
    if (respectVisibility && this.visibilityState === "hidden") return Promise.resolve(undefined);

    const task = this.runSingleFlight(entry.key, owner.task);
    void task.then(
      () => this.schedule(entry, expectedGeneration),
      () => this.schedule(entry, expectedGeneration),
    );
    return task;
  }

  private schedule(entry: RefreshEntry, expectedGeneration: number): void {
    if (
      this.entries.get(entry.key) !== entry
      || entry.owners.size === 0
      || entry.generation !== expectedGeneration
      || this.visibilityState === "hidden"
    ) return;

    const owner = entry.owners.values().next().value as RefreshOwner | undefined;
    const delay = owner?.fallbackDelayMs?.() ?? null;
    if (delay === null) return;

    entry.timer = this.windowTarget.setTimeout(() => {
      entry.timer = undefined;
      if (entry.generation !== expectedGeneration || this.visibilityState === "hidden") return;
      const currentOwnerId = entry.owners.keys().next().value as symbol | undefined;
      if (!currentOwnerId) return;
      const currentOwner = entry.owners.get(currentOwnerId);
      if (!currentOwner) return;
      const task = this.runSingleFlight(entry.key, currentOwner.task);
      void task.then(
        () => this.schedule(entry, expectedGeneration),
        () => this.schedule(entry, expectedGeneration),
      );
    }, Math.max(0, delay));
  }

  private runSingleFlight<T>(key: string, task: () => Promise<T>): Promise<T> {
    const current = this.inFlight.get(key);
    if (current) return current as Promise<T>;

    const next = Promise.resolve().then(task);
    this.inFlight.set(key, next);
    void next.then(
      () => {
        if (this.inFlight.get(key) === next) this.inFlight.delete(key);
      },
      () => {
        if (this.inFlight.get(key) === next) this.inFlight.delete(key);
      },
    );
    return next;
  }

  private release(entry: RefreshEntry, ownerId: symbol): void {
    entry.owners.delete(ownerId);
    if (entry.owners.size > 0) return;
    this.stopEntry(entry);
    this.entries.delete(entry.key);
  }

  private stopEntry(entry: RefreshEntry): void {
    entry.generation += 1;
    this.clearTimer(entry);
  }

  private clearTimer(entry: RefreshEntry): void {
    if (entry.timer === undefined) return;
    this.windowTarget.clearTimeout(entry.timer);
    entry.timer = undefined;
  }
}

export const appRefreshScheduler = new KeyedRefreshScheduler();
