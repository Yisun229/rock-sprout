// 顶部黄条：这次的数据是断网时从缓存里翻出来的，可能已经过期。
import { t, whenText } from "../i18n";

type Props = {
  cachedAt: number | null;
  busy: boolean;
  onRetry: () => void;
};

export default function StaleBanner({ cachedAt, busy, onRetry }: Props) {
  return (
    <div className="banner warn">
      <span>{t("mine.staleBanner", { when: whenText(cachedAt) })}</span>
      <button className="btn small ghost" disabled={busy} onClick={onRetry}>
        {busy ? t("action.retrying") : t("action.retry")}
      </button>
    </div>
  );
}
