// 东西还没出来的时候先摆几块灰条。不用转圈：转圈看不出等的是什么，骨架一眼就知道「马上要出一列」。
import { t } from "../../i18n";

type Props = {
  /** 摆几行，默认 6 行（一屏能看到的条数） */
  rows?: number;
  /** 挤在卡片里用，行高矮一些（比如设置页体检卡第一次查的时候） */
  compact?: boolean;
};

export default function SkeletonRows({ rows = 6, compact = false }: Props) {
  return (
    <div className={compact ? "skeleton compact" : "skeleton"} role="status" aria-label={t("state.loading")}>
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="skeleton-row">
          <span className="skeleton-bar skeleton-title" />
          <span className="skeleton-bar skeleton-sub" />
        </div>
      ))}
    </div>
  );
}
