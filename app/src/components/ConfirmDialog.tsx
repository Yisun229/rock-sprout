// 动手之前先问一句的弹层。
//
// 用在两处：批量卸载（列出到底要卸哪几个）、装之前的冲突提醒（说清会盖掉谁）。
// 样式上是一块浮在内容上的卡片，不拦键盘、不搞动画——这个程序的调性就是朴素、看得清字。
import { t } from "../i18n";

type Props = {
  title: string;
  /** 一行一句，直接照顺序列出来 */
  lines: string[];
  confirmLabel: string;
  cancelLabel: string;
  /** 点了确认之后正在忙，按钮先别让再点 */
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
};

export default function ConfirmDialog({
  title,
  lines,
  confirmLabel,
  cancelLabel,
  busy,
  onConfirm,
  onCancel,
}: Props) {
  return (
    <div className="modal">
      <div className="modal-card">
        <div className="modal-title">{title}</div>
        <ul className="modal-list">
          {lines.map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
        <div className="modal-actions">
          <button className="btn ghost small" disabled={busy} onClick={onCancel}>
            {cancelLabel}
          </button>
          <button className="btn small" disabled={busy} onClick={onConfirm}>
            {busy ? t("action.working") : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
