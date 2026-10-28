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
  onClear: () => void;
};

export default function BulkBar({ count, busy, onInstall, onRemove, onClear }: Props) {
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
        <button className="btn ghost small" disabled={busy} onClick={onClear}>
          {t("bulk.clear")}
        </button>
      </div>
    </div>
  );
}
