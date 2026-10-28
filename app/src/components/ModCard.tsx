// 矿脉里的一行：勾选框、名字、简介、下载量、装按钮。
// 只管长什么样，不自己发请求——点「装」交给上层的 `onInstall`。
import type { ModSummary } from "../api";
import { t, thousands } from "../i18n";

type Props = {
  mod: ModSummary;
  /** 这一个正在装 */
  installing: boolean;
  /** 有别的东西正在装，按钮先别点 */
  disabled: boolean;
  onInstall: () => void;
  /** 这一行被勾上了 */
  selected: boolean;
  /** 点勾选框；`shift` 为真表示要连选一段 */
  onToggle: (shift: boolean) => void;
};

export default function ModCard({
  mod,
  installing,
  disabled,
  onInstall,
  selected,
  onToggle,
}: Props) {
  return (
    <div className={selected ? "row picked" : "row"}>
      <input
        type="checkbox"
        className="pick"
        aria-label={mod.name}
        checked={selected}
        onChange={() => undefined}
        onClick={(e) => onToggle(e.shiftKey)}
      />
      <div className="row-main">
        <div className="row-title">{mod.name}</div>
        {mod.summary && <div className="row-sub">{mod.summary}</div>}
      </div>
      <div className="row-side">
        <span className="badge">{t("mine.downloads", { count: thousands(mod.downloads) })}</span>
        <button className="btn small" disabled={disabled} onClick={onInstall}>
          {installing ? t("action.installing") : t("action.install")}
        </button>
      </div>
    </div>
  );
}
