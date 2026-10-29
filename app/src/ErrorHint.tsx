// 行内提示：某一张卡、某一行出错时用，红点 + 一句话 + 重试，不占整块地方。
// 整块内容出不来（比如整页搜不出东西）用 components/states/ErrorState。
//
// 三种最常见的情况（没令牌 / 连不上 / 被限速）各有各的说法，别让用户看同一句「请求失败」。
// 具体文案在 locales/zh-CN.ts 里，这里只负责挑哪一组。
import type { ApiError } from "./api";
import { errorCopy } from "./components/states/ErrorState";
import { t } from "./i18n";

type Props = {
  error: ApiError;
  onRetry?: () => void;
  onGoSettings?: () => void;
};

export default function ErrorHint({ error, onRetry, onGoSettings }: Props) {
  const { title, advice, needsSettings } = errorCopy(error);

  return (
    <p className="hint-err">
      <span className="hint-dot" aria-hidden="true" />
      <span className="hint-text">
        {title}：{advice}
      </span>
      <span className="hint-detail">{t("error.detail", { message: error.message })}</span>
      {onRetry && (
        <button className="hint-action" onClick={onRetry}>
          {t("action.retry")}
        </button>
      )}
      {needsSettings && onGoSettings && (
        <button className="hint-action" onClick={onGoSettings}>
          {t("action.goSettings")}
        </button>
      )}
    </p>
  );
}
