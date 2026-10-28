// 背包里的一行：勾选框、名字、文件名（或者「原文件已删除」）、卸载按钮。
//
// 原文件已经没了的那种，按钮是「仅删除记录」：只清掉这条空记账，游戏目录一个字节都不碰。
import type { InstalledMod } from "../api";
import { t } from "../i18n";
import ConflictBadge from "./ConflictBadge";

type Props = {
  mod: InstalledMod;
  busy: boolean;
  /** 有几个文件被别人盖住了（0 表示没有） */
  overridden: number;
  /** 有几个文件盖住了别人（0 表示没有） */
  covers: number;
  /** 正常卸载（会删缓存里的包、重建合并包） */
  onRemove: () => void;
  /** 只删掉这条空记账（文件早就不在了） */
  onDropRecord: () => void;
  /** 这一行被勾上了 */
  selected: boolean;
  /** 点勾选框；`shift` 为真表示要连选一段 */
  onToggle: (shift: boolean) => void;
};

export default function ModRow({
  mod,
  busy,
  overridden,
  covers,
  onRemove,
  onDropRecord,
  selected,
  onToggle,
}: Props) {
  const classes = ["row"];
  if (mod.missing) classes.push("missing");
  if (selected) classes.push("picked");
  return (
    <li className={classes.join(" ")}>
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
        <div className="row-sub">
          {mod.missing ? t("bag.missing") : t("bag.file", { name: mod.version })}
        </div>
      </div>
      <div className="row-side">
        <ConflictBadge overridden={overridden} covers={covers} />
        <button
          className="btn ghost small"
          disabled={busy}
          onClick={mod.missing ? onDropRecord : onRemove}
        >
          {mod.missing ? t("action.dropRecord") : t("action.uninstall")}
        </button>
      </div>
    </li>
  );
}
