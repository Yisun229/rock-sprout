// 底部常驻条：批量任务在跑（或刚跑完）时出现。
//
// 不挡操作、可以收起；没成的会标红并写明原因，可以只重试没成的。
import { t, type TextKey } from "../i18n";
import type { Queue } from "../hooks/useQueue";

type Props = {
  queue: Queue;
};

const STATUS_KEY: Record<string, TextKey> = {
  queued: "queue.status.queued",
  prechecking: "queue.status.prechecking",
  downloading: "queue.status.downloading",
  merging: "queue.status.merging",
  done: "queue.status.done",
  failed: "queue.status.failed",
  cancelled: "queue.status.cancelled",
};

export default function QueueBar({ queue }: Props) {
  if (queue.items.length === 0) return null;

  const total = queue.items.length;
  const done = queue.items.filter((it) => it.status === "done").length;
  const failed = queue.items.filter((it) => it.status === "failed").length;

  const title = queue.running
    ? t("queue.running", { done: done + failed, total })
    : failed > 0
      ? t("queue.failed", { count: failed })
      : t("queue.done");

  return (
    <div className={failed > 0 ? "bulkbar queue bad" : "bulkbar queue"}>
      <div className="queue-head">
        <span className="bulkbar-count">{title}</span>
        <div className="searchbar">
          {queue.running && (
            <button className="btn ghost small" onClick={queue.cancel}>
              {t("queue.cancel")}
            </button>
          )}
          {!queue.running && failed > 0 && (
            <button className="btn small" onClick={queue.retryFailed}>
              {t("queue.retry")}
            </button>
          )}
          <button
            className="btn ghost small"
            onClick={() => queue.setCollapsed(!queue.collapsed)}
          >
            {queue.collapsed ? t("queue.expand") : t("queue.collapse")}
          </button>
          {!queue.running && (
            <button className="btn ghost small" onClick={queue.dismiss}>
              {t("queue.close")}
            </button>
          )}
        </div>
      </div>

      {!queue.collapsed && (
        <ul className="queue-list">
          {queue.items.map((item) => (
            <li
              key={item.key}
              className={item.status === "failed" ? "queue-item bad" : "queue-item"}
            >
              <span className="queue-name">{item.name}</span>
              <span className="queue-status">{t(STATUS_KEY[item.status])}</span>
              {item.conflicts ? (
                <span className="badge">{t("queue.conflicts", { count: item.conflicts })}</span>
              ) : null}
              {item.message ? <span className="queue-msg">{item.message}</span> : null}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
