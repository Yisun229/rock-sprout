// 出错时统一长这样：一句「哪里出了问题」+ 一句「你现在该做什么」+ 可点的动作。
// 三种最常见的情况（没令牌 / 连不上 / 被限速）各有各的说法，别让用户看同一句「请求失败」。
//
// 具体文案在 locales/zh-CN.ts 里，这里只负责挑哪一组。
import type { ApiError } from "./api";
import { t, type TextKey } from "./i18n";

type Props = {
  error: ApiError;
  onRetry?: () => void;
  onGoSettings?: () => void;
};

/** 这些错误码的提示里要带「去设置页」按钮。 */
const NEED_SETTINGS = new Set(["no_token", "unauthorized"]);

/** 有专属说法的错误码；其他的走通用说法。 */
const KNOWN = new Set([
  "no_token",
  "unauthorized",
  "rate_limited",
  "network",
  "integrate",
  "not_installed",
  "game_running",
]);

export default function ErrorHint({ error, onRetry, onGoSettings }: Props) {
  const known = KNOWN.has(error.code);
  const title = t(known ? (`error.${error.code}.title` as TextKey) : "error.title");
  const advice = t(known ? (`error.${error.code}.advice` as TextKey) : "error.advice");

  return (
    <div className="card fail">
      <div className="card-title fail-title">{title}</div>
      <p className="note">{advice}</p>
      <p className="note small dim">{t("error.detail", { message: error.message })}</p>
      <div className="searchbar">
        {onRetry && (
          <button className="btn small" onClick={onRetry}>
            {t("action.retry")}
          </button>
        )}
        {NEED_SETTINGS.has(error.code) && onGoSettings && (
          <button className="btn ghost small" onClick={onGoSettings}>
            {t("action.goSettings")}
          </button>
        )}
      </div>
    </div>
  );
}
