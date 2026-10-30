// 底部批量操作条：选中若干 mod 之后出现。
//
// 两个页面共用一个条：矿脉传 `onInstall`，背包传 `onRemove`，没传的那个按钮不显示。
import { t } from "../i18n";

type Props = {
  /** 选了几个 */
  count: number;
  busy: boolean;
  onInstall?: () => void;
  onRemove?: () => void;
  /** 把选中的简介翻成中文（只有矿脉传） */
  onTranslate?: () => void;
  /** 批量翻译的进度；没在跑就是 null */
  translateProgress?: { done: number; total: number } | null;
  onClear: () => void;
};

export default function BulkBar({
  count,
  busy,
  onInstall,
  onRemove,
  onTranslate,
  translateProgress,
  onClear,
}: Props) {
  if (count === 0) return null;
  return (
    <div className="bulkbar">
      <span className="bulkbar-count">{t("bulk.count", { count })}</span>
      <div className="searchbar">
        {onInstall && (
          <button className="btn small" disabled={busy} onClick={onInstall}>
            {t("bulk.install")}
          </button>
        )}
        {onRemove && (
          <button className="btn ghost small" disabled={busy} onClick={onRemove}>
            {t("bulk.remove")}
          </button>
        )}
        {onTranslate && (
          <button
            className="btn ghost small"
            disabled={busy || translateProgress !== null}
            onClick={onTranslate}
          >
            {translateProgress
              ? t("bulk.translating", {
                  done: translateProgress.done,
                  total: translateProgress.total,
                })
              : t("bulk.translate", { count })}
          </button>
        )}
        <button className="btn ghost small" disabled={busy} onClick={onClear}>
          {t("bulk.clear")}
        </button>
      </div>
    </div>
  );
}
