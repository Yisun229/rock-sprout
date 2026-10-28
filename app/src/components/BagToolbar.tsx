// 背包页顶上那排按钮：刷新、导入文件、导入文件夹，外加冲突相关的两个入口。
import { t } from "../i18n";

type Props = {
  busy: boolean;
  /** 有冲突时才显示「看看谁盖了谁」和「只看有冲突的」 */
  hasConflicts: boolean;
  onlyConflicts: boolean;
  onRefresh: () => void;
  onPickFiles: () => void;
  onPickFolder: () => void;
  onOpenDrawer: () => void;
  onToggleOnlyConflicts: (next: boolean) => void;
};

export default function BagToolbar({
  busy,
  hasConflicts,
  onlyConflicts,
  onRefresh,
  onPickFiles,
  onPickFolder,
  onOpenDrawer,
  onToggleOnlyConflicts,
}: Props) {
  return (
    <div className="searchbar">
      <button className="btn ghost" disabled={busy} onClick={onRefresh}>
        {busy ? t("action.reading") : t("action.refresh")}
      </button>
      <button className="btn ghost" disabled={busy} onClick={onPickFiles}>
        {t("action.importFiles")}
      </button>
      <button className="btn ghost" disabled={busy} onClick={onPickFolder}>
        {t("action.importFolder")}
      </button>
      {hasConflicts && (
        <>
          <button className="btn ghost" onClick={onOpenDrawer}>
            {t("conflict.openDrawer")}
          </button>
          <label className="checkline">
            <input
              type="checkbox"
              className="pick"
              checked={onlyConflicts}
              onChange={(e) => onToggleOnlyConflicts(e.target.checked)}
            />
            {t("conflict.onlyConflicts")}
          </label>
        </>
      )}
    </div>
  );
}
