// 整块内容出不来时用这个：一句「哪里出了问题」+ 一句「你现在该做什么」+ 可点的动作。
// 只是某一行/某一张卡出错，用行内的 ErrorHint，别占整块地方。
//
// 三种最常见的情况（没令牌 / 连不上 / 被限速）各有各的说法，别让用户看同一句「请求失败」。
// 具体文案在 locales/zh-CN.ts 里，这里只负责挑哪一组。
//
// 按钮也不是每个错误都给「重试」：
// - 令牌没填/失效：只能去设置（给重试就是耍人，重试换不来令牌）
// - 游戏正在跑：只能说「知道了，关掉游戏再来」
// - 网络/超时/限速/装的途中出错：重试
// - 其余不认识的要求：复制诊断 + 重试（让人能把错误码贴出来）
import type { ApiError } from "../../api";
import { APP_TITLE } from "../../brand";
import { t, type TextKey } from "../../i18n";
import { showToast } from "../../toast";

/** 这些错误码只能引导去设置页，重试没用。 */
const NEED_SETTINGS = new Set(["no_token", "unauthorized"]);

/** 有专属说法的错误码；其他的走通用说法。 */
// 规矩：人话标题由错误类型直接决定，已知类型一律走自己的文案，
// 不许用一句通用兜底去盖住它——「找不到游戏」被说成「游戏正开着」就是这么来的。
const KNOWN = new Set([
  "no_token",
  "unauthorized",
  "rate_limited",
  "network",
  "integrate",
  "not_installed",
  "game_running",
  "no_game",
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
  /** 「游戏正在跑」这类只能认下来的提示：给了才出「知道了」。 */
  onDismiss?: () => void;
};

/** 把这次错误抄成一段能直接贴出来的文字。 */
export function diagnosticsText(error: ApiError) {
  return [
    APP_TITLE,
    `时间：${new Date().toISOString()}`,
    `错误码：${error.code}`,
    `说法：${error.message}`,
  ].join("\n");
}

export default function ErrorState({ error, onRetry, onGoSettings, onDismiss }: Props) {
  const { title, advice, needsSettings } = errorCopy(error);
  const gameRunning = error.code === "game_running";
  const unknown = !KNOWN.has(error.code);
  // 令牌这类错误不给重试；游戏在跑也不给重试——重试还是会被同一个进程挡住
  const showRetry = Boolean(onRetry) && !needsSettings && !gameRunning;

  const copyDiagnostics = async () => {
    try {
      await navigator.clipboard.writeText(diagnosticsText(error));
      showToast(t("error.copied"));
    } catch {
      showToast(t("error.copyFailed"));
    }
  };

  return (
    <div className="card fail" data-block="error-state">
      <div className="card-title fail-title">{title}</div>
      <p className="note">{advice}</p>
      <p className="note small dim">{t("error.detail", { message: error.message })}</p>
      <div className="searchbar">
        {gameRunning && onDismiss && (
          <button className="btn small" onClick={onDismiss}>
            {t("action.gotIt")}
          </button>
        )}
        {needsSettings && onGoSettings && (
          <button className="btn small" onClick={onGoSettings}>
            {t("action.goSettings")}
          </button>
        )}
        {showRetry && (
          <button className="btn small" onClick={onRetry}>
            {t("action.retry")}
          </button>
        )}
        {unknown && (
          <button className="btn ghost small" onClick={() => void copyDiagnostics()}>
            {t("action.copyDiagnostics")}
          </button>
        )}
      </div>
    </div>
  );
}
