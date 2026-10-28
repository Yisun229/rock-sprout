// 矿脉页的翻页条。
import { t } from "../i18n";

type Props = {
  page: number;
  hasPrev: boolean;
  hasNext: boolean;
  busy: boolean;
  onPrev: () => void;
  onNext: () => void;
};

export default function Pager({ hasPrev, hasNext, busy, onPrev, onNext }: Props) {
  return (
    <div className="pager">
      <button className="btn ghost" disabled={busy || !hasPrev} onClick={onPrev}>
        {t("action.prevPage")}
      </button>
      <button className="btn ghost" disabled={busy || !hasNext} onClick={onNext}>
        {t("action.nextPage")}
      </button>
    </div>
  );
}
