// 批量操作的执行队列。
//
// 规矩（来自 v0.2 方案）：
// - 下载 / 导入可以重叠，2 路并发；
// - **写进游戏那一步严格串行**（同一个游戏目录，并发会踩）；
// - 失败不整体回滚：成了的留下，没成的各自记原因，可以只重试没成的；
// - 取消只停后面的，正在做的那一项做完就停。
import { useCallback, useRef, useState } from "react";
import { call, toApiError, type PrecheckReport } from "../api";

/** 并发下载的路数。合并那一步无论如何都是排队跑的。 */
const CONCURRENCY = 2;

export type QueueStatus =
  | "queued"
  | "prechecking"
  | "downloading"
  | "merging"
  | "done"
  | "failed"
  | "cancelled";

export type QueueItem = {
  key: string;
  /** install = 从 mod.io 装；import = 本地文件；remove = 卸载 */
  kind: "install" | "import" | "remove";
  id?: number;
  path?: string;
  name: string;
  status: QueueStatus;
  /** 失败原因（中文） */
  message?: string;
  /** 预检发现会撞几处 */
  conflicts?: number;
};

export type NewQueueItem = {
  kind: "install" | "import" | "remove";
  id?: number;
  path?: string;
  name: string;
};

export type Queue = {
  items: QueueItem[];
  running: boolean;
  collapsed: boolean;
  setCollapsed: (collapsed: boolean) => void;
  start: (items: NewQueueItem[]) => void;
  cancel: () => void;
  retryFailed: () => void;
  dismiss: () => void;
};

function keyOf(item: NewQueueItem): string {
  if (item.kind === "install") return `install:${item.id}`;
  if (item.kind === "remove") return `remove:${item.id}`;
  return `import:${item.path}`;
}

/** 队列跑完之后刷一下已装列表用（可选）。 */
export function useQueue(onFinished?: () => void): Queue {
  const [items, setItems] = useState<QueueItem[]>([]);
  const [running, setRunning] = useState(false);
  const [collapsed, setCollapsed] = useState(false);
  const cancelled = useRef(false);
  // 合并必须一个接一个：把调用串成一条链，前一个结束才轮到下一个
  const mergeChain = useRef<Promise<unknown>>(Promise.resolve());

  const patch = useCallback((key: string, changes: Partial<QueueItem>) => {
    setItems((list) => list.map((it) => (it.key === key ? { ...it, ...changes } : it)));
  }, []);

  const withMergeLock = useCallback(<T,>(task: () => Promise<T>): Promise<T> => {
    const next = mergeChain.current.then(task, task);
    mergeChain.current = next.catch(() => undefined);
    return next;
  }, []);

  const runOne = useCallback(
    async (item: QueueItem) => {
      try {
        if (item.kind === "remove") {
          patch(item.key, { status: "merging" });
          await call("uninstall_mod", { id: item.id });
          patch(item.key, { status: "done" });
          return;
        }

        patch(item.key, { status: "prechecking" });
        const target = item.kind === "install" ? String(item.id) : (item.path as string);
        let conflicts = 0;
        try {
          const report = await call<PrecheckReport>("precheck_install", { modIds: [target] });
          conflicts = report.conflicts.length;
        } catch {
          // 预检只是提前打个招呼，它自己出错不该挡着装
        }

        if (cancelled.current) {
          patch(item.key, { status: "cancelled", conflicts });
          return;
        }

        patch(item.key, { status: "downloading", conflicts });
        if (item.kind === "install") {
          await call("download_mod", { id: item.id });
        }

        await withMergeLock(async () => {
          patch(item.key, { status: "merging" });
          if (item.kind === "install") {
            await call("apply_all");
          } else {
            await call("import_local", { path: item.path });
          }
        });
        patch(item.key, { status: "done" });
      } catch (e) {
        const err = toApiError(e);
        patch(item.key, { status: "failed", message: err.message });
      }
    },
    [patch, withMergeLock],
  );

  const runAll = useCallback(
    async (list: QueueItem[]) => {
      cancelled.current = false;
      setRunning(true);
      const pending = [...list];

      const worker = async () => {
        while (pending.length > 0) {
          const item = pending.shift();
          if (!item) return;
          if (cancelled.current) {
            patch(item.key, { status: "cancelled" });
            continue;
          }
          await runOne(item);
        }
      };

      await Promise.all(Array.from({ length: Math.min(CONCURRENCY, list.length) }, worker));
      setRunning(false);
      onFinished?.();
    },
    [onFinished, patch, runOne],
  );

  const start = useCallback(
    (incoming: NewQueueItem[]) => {
      if (incoming.length === 0) return;
      const fresh: QueueItem[] = incoming.map((item) => ({
        ...item,
        key: keyOf(item),
        status: "queued",
      }));
      setItems(fresh);
      setCollapsed(false);
      void runAll(fresh);
    },
    [runAll],
  );

  const cancel = useCallback(() => {
    cancelled.current = true;
  }, []);

  const retryFailed = useCallback(() => {
    const failed = items.filter((it) => it.status === "failed");
    if (failed.length === 0) return;
    const reset = failed.map((it) => ({ ...it, status: "queued" as QueueStatus, message: undefined }));
    setItems((list) => list.map((it) => reset.find((r) => r.key === it.key) ?? it));
    setCollapsed(false);
    void runAll(reset);
  }, [items, runAll]);

  const dismiss = useCallback(() => {
    if (running) return;
    setItems([]);
  }, [running]);

  return {
    items,
    running,
    collapsed,
    setCollapsed,
    start,
    cancel,
    retryFailed,
    dismiss,
  };
}
