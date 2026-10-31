// 脚本类 mod 撞上同名目录时问一句：换个名字，还是把旧的替换掉。
//
// 「替换」不是删——旧目录会先整个挪到我们自己的备份目录，所以两条路都能回头。
import { useState } from "react";
import { t } from "../i18n";

type Props = {
  /** 后端给的原话，说清是哪个目录重了 */
  message: string;
  /** 默认名字（拿文件名凑的），用户可以直接改 */
  defaultName: string;
  busy?: boolean;
  onRename: (name: string) => void;
  onReplace: () => void;
  onCancel: () => void;
};

export default function NameConflictDialog({
  message,
  defaultName,
  busy,
  onRename,
  onReplace,
  onCancel,
}: Props) {
  const [name, setName] = useState(defaultName);
  const trimmed = name.trim();

  return (
    <div className="modal">
      <div className="modal-card" data-block="name-conflict">
        <div className="modal-title">{t("bag.nameConflictTitle")}</div>
        <p className="modal-note">{message}</p>
        <label className="modal-field">
          <span className="dim small">{t("bag.nameConflictLabel")}</span>
          <input
            className="input"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={defaultName}
          />
        </label>
        <div className="modal-actions">
          <button className="btn ghost small" disabled={busy} onClick={onCancel}>
            {t("bag.nameConflictCancel")}
          </button>
          <button className="btn ghost small" disabled={busy} onClick={onReplace}>
            {t("bag.nameConflictReplace")}
          </button>
          <button
            className="btn small"
            disabled={busy || trimmed.length === 0}
            onClick={() => onRename(trimmed)}
          >
            {t("bag.nameConflictRename")}
          </button>
        </div>
      </div>
    </div>
  );
}
