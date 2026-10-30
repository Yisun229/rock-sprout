// 矿脉里的一行：勾选框、名字、简介、下载量、装按钮。
// 只管长什么样，不自己发请求——点「装」交给上层的 `onInstall`，点行本身交给 `onOpen`。
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
  /** 点行（名字那一片）→ 右边开预览 */
  onOpen: () => void;
  /** 右边正开着这个 mod 的预览 */
  opened: boolean;
};

export default function ModCard({
  mod,
  installing,
  disabled,
  onInstall,
  selected,
  onToggle,
  onOpen,
  opened,
}: Props) {
  const cls = ["row"];
  if (selected) cls.push("picked");
  if (opened) cls.push("opened");

  return (
    <div className={cls.join(" ")} data-mod-id={mod.id} onClick={onOpen}>
      <input
        type="checkbox"
        className="pick"
        aria-label={mod.name}
        checked={selected}
        onChange={() => undefined}
        // 勾选不该顺带把预览打开
        onClick={(e) => {
          e.stopPropagation();
          onToggle(e.shiftKey);
        }}
      />
      <div className="row-main">
        <div className="row-title">{mod.name}</div>
        {mod.summary && <div className="row-sub">{mod.summary}</div>}
      </div>
      <div className="row-side">
        <span className="badge">{t("mine.downloads", { count: thousands(mod.downloads) })}</span>
        <button
          className="btn small"
          disabled={disabled}
          // 点「装」不该顺带打开预览
          onClick={(e) => {
            e.stopPropagation();
            onInstall();
          }}
        >
          {installing ? t("action.installing") : t("action.install")}
        </button>
      </div>
    </div>
  );
}
