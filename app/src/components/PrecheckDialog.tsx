// 装之前的冲突提醒：把这次要装的几个合起来算一遍，只弹一次。
//
// 说的都是后果，不说「覆盖率」「优先级」这类词：用户只想知道「我装完还能看到原来那样吗」。
import type { PrecheckReport } from "../api";
import { t } from "../i18n";
import ConfirmDialog from "./ConfirmDialog";

type Props = {
  report: PrecheckReport;
  /** 一次要装几个 */
  count: number;
  onConfirm: () => void;
  onCancel: () => void;
};

/** 最多列出前几处具体文件，再多就只给总数——不是让人一条条看，是让人知道「确实撞了」。 */
const MAX_LINES = 5;

export default function PrecheckDialog({ report, count, onConfirm, onCancel }: Props) {
  const lines = [report.summary];
  for (const conflict of report.conflicts.slice(0, MAX_LINES)) {
    lines.push(conflict.path);
  }
  if (report.conflicts.length > MAX_LINES) {
    lines.push(t("conflict.precheckMore", { count: report.conflicts.length - MAX_LINES }));
  }
  for (const warning of report.warnings.slice(0, 3)) {
    lines.push(warning);
  }

  return (
    <ConfirmDialog
      title={
        report.conflicts.length > 0
          ? t("conflict.precheckTitle")
          : t("conflict.precheckClean")
      }
      lines={lines}
      confirmLabel={t("conflict.precheckGo", { count })}
      cancelLabel={t("conflict.precheckCancel")}
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
}
