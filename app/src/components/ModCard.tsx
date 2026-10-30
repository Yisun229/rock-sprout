// 矿脉里的一行：勾选框、名字、简介、下载量、译、装按钮。
// 只管长什么样，不自己发请求——点「装」交给上层的 `onInstall`，点行本身交给 `onOpen`，
// 点「译」交给 `onTranslate`（译文由上层存在内存里，翻过一次切页再回来还在）。
import type { ModSummary } from "../api";
import { t, thousands } from "../i18n";

type Props = {
  mod: ModSummary;
  /** 这一个正在装 */
  installing: boolean;
  /** 有别的东西正在装，按钮先别点 */
  disabled: boolean;
  onInstall: () => void;
  /** 这一行被勾上了 */
  selected: boolean;
  /** 点勾选框；`shift` 为真表示要连选一段 */
  onToggle: (shift: boolean) => void;
  /** 点行（名字那一片）→ 右边开预览 */
  onOpen: () => void;
  /** 右边正开着这个 mod 的预览 */
  opened: boolean;
  /** 简介的中文；没翻过就是 null */
  translated?: string | null;
  /** 正在翻这一行 */
  translating?: boolean;
  /** 翻失败的原因；有值就把按钮变灰、鼠标停上去能看见 */
  translateError?: string | null;
  /** 点「译」（再点一次收起译文） */
  onTranslate?: () => void;
};

export default function ModCard({
  mod,
  installing,
  disabled,
  onInstall,
  selected,
  onToggle,
  onOpen,
  opened,
  translated,
  translating,
  translateError,
  onTranslate,
}: Props) {
  const zh = translated && translated.trim() ? translated : null;

  const cls = ["row"];
  if (selected) cls.push("picked");
  if (opened) cls.push("opened");
  // 翻过译的行多一行原文，得把三行压紧才塞得进 72px
  if (zh) cls.push("has-tr");
  const trCls = ["btn", "ghost", "small", "translate"];
  if (translateError) trCls.push("failed");
  if (zh) trCls.push("on");

  return (
    <div className={cls.join(" ")} data-mod-id={mod.id} onClick={onOpen}>
      <input
        type="checkbox"
        className="pick"
        aria-label={mod.name}
        checked={selected}
        onChange={() => undefined}
        // 勾选不该顺带把预览打开
        onClick={(e) => {
          e.stopPropagation();
          onToggle(e.shiftKey);
        }}
      />
      <div className="row-main">
        <div className="row-title">{mod.name}</div>
        {zh ? (
          <>
            <div className="row-sub">{zh}</div>
            {/* 原文留着：译文有偏差时还能对一眼 */}
            <div className="row-orig">{mod.summary}</div>
          </>
        ) : (
          mod.summary && <div className="row-sub">{mod.summary}</div>
        )}
      </div>
      <div className="row-side">
        <span className="badge">{t("mine.downloads", { count: thousands(mod.downloads) })}</span>
        {onTranslate && (
          <button
            className={trCls.join(" ")}
            disabled={translating}
            title={translateError ?? (zh ? t("translate.hide") : t("translate.one"))}
            aria-label={t("translate.one")}
            // 点「译」不该顺带打开预览
            onClick={(e) => {
              e.stopPropagation();
              onTranslate();
            }}
          >
            {translating ? t("translate.doing") : t("translate.one")}
          </button>
        )}
        <button
          className="btn small"
          disabled={disabled}
          // 点「装」不该顺带打开预览
          onClick={(e) => {
            e.stopPropagation();
            onInstall();
          }}
        >
          {installing ? t("action.installing") : t("action.install")}
        </button>
      </div>
    </div>
  );
}
