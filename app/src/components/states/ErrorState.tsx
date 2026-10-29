// 整块内容出不来时用这个：一句「哪里出了问题」+ 一句「你现在该做什么」+ 可点的动作。
// 只是某一行/某一张卡出错，用行内的 ErrorHint，别占整块地方。
//
// 三种最常见的情况（没令牌 / 连不上 / 被限速）各有各的说法，别让用户看同一句「请求失败」。
// 具体文案在 locales/zh-CN.ts 里，这里只负责挑哪一组。
import type { ApiError } from "../../api";
import { t, type TextKey } from "../../i18n";

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

/** 挑出这个错误该说哪几句。整块的错误态和行内提示共用这一份，免得两处说法跑偏。 */
export function errorCopy(error: ApiError) {
  const known = KNOWN.has(error.code);
  return {
    title: t(known ? (`error.${error.code}.title` as TextKey) : "error.title"),
    advice: t(known ? (`error.${error.code}.advice` as TextKey) : "error.advice"),
    needsSettings: NEED_SETTINGS.has(error.code),
  };
}

type Props = {
  error: ApiError;
  onRetry?: () => void;
  onGoSettings?: () => void;
};

export default function ErrorState({ error, onRetry, onGoSettings }: Props) {
  const { title, advice, needsSettings } = errorCopy(error);

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
        {needsSettings && onGoSettings && (
          <button className="btn ghost small" onClick={onGoSettings}>
            {t("action.goSettings")}
          </button>
        )}
      </div>
    </div>
  );
}
