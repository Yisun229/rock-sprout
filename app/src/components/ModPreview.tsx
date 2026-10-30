// 抽屉预览：点矿脉里的一行，右边开一块面板看详情。
//
// 只管长什么样和怎么取数据；「装」还是交给矿脉那套预检 + 排队（`onInstall`）。
// 宽屏是右边一栏，窄屏（<1000px）自动变成盖在列表上的浮层，带一层遮罩。
import { useEffect, useRef, useState } from "react";
import {
  getModDetail,
  toApiError,
  type ApiError,
  type ModDetail,
  type ModSummary,
} from "../api";
import { t, thousands, whenText } from "../i18n";

/** 标签最多显示这么多个，多出来的折成「还有 N 个」 */
const TAG_LIMIT = 6;

type Props = {
  /** 打开的是哪个 mod */
  id: number;
  onClose: () => void;
  /** 装这个 mod：交给矿脉那套预检 + 排队 */
  onInstall: (mod: ModSummary) => void;
  /** 正在装的是哪个；不是这个就别让点 */
  installing: number | null;
  /** 这一行已经装过了，按钮换成「已安装」 */
  installed?: boolean;
};

export default function ModPreview({
  id,
  onClose,
  onInstall,
  installing,
  installed = false,
}: Props) {
  const [detail, setDetail] = useState<ModDetail | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const [expanded, setExpanded] = useState(false);
  /** 封面图挂了（断网、CDN 被挡）就退回色块，别留一个破图标 */
  const [coverFailed, setCoverFailed] = useState(false);
  // 点「重试」时加一，effect 重跑
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let alive = true;
    setDetail(null);
    setError(null);
    setExpanded(false);
    setCoverFailed(false);
    getModDetail(id)
      .then((d) => {
        if (alive) setDetail(d);
      })
      .catch((e) => {
        if (alive) setError(toApiError(e));
      });
    return () => {
      alive = false;
    };
  }, [id, attempt]);

  // Esc 关掉。监听器一辈子只挂一次，用 ref 记住最新的 onClose，
  // 免得父组件每次渲染换个函数就把监听器拆了重挂（关着再开之后容易漏事件）。
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeRef.current();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const cover = detail ? detail.logo_640 || detail.logo_320 : "";
  const text = detail ? detail.description_plaintext || detail.summary : "";
  const tags = detail?.tags ?? [];
  const extra = Math.max(0, tags.length - TAG_LIMIT);
  // 简介短的时候不用「展开全文」——按钮比内容还长
  const longText = text.length > 160;

  return (
    <>
      <div className="preview-mask" onClick={onClose} />
      <aside
        className="mod-preview"
        data-testid="mod-preview"
        role="dialog"
        aria-label={detail?.name ?? t("preview.loading")}
      >
        <button className="btn ghost small preview-close" onClick={onClose} aria-label={t("preview.close")}>
          ×
        </button>

        {!detail && !error && (
          <div className="preview-skel" aria-label={t("preview.loading")}>
            <div className="sk-cover" />
            <div className="sk-line w60" />
            <div className="sk-line w40" />
            <div className="sk-line w90" />
            <div className="sk-line w90" />
            <div className="sk-line w70" />
          </div>
        )}

        {error && (
          <div className="preview-fail">
            <p>{t("preview.failed")}</p>
            <p className="dim small">{error.message}</p>
            <button className="btn small" onClick={() => setAttempt((n) => n + 1)}>
              {t("preview.retry")}
            </button>
          </div>
        )}

        {detail && (
          <>
            {cover && !coverFailed ? (
              <img
                className="preview-cover"
                src={cover}
                alt=""
                loading="lazy"
                onError={() => setCoverFailed(true)}
              />
            ) : (
              // 没封面、或者图根本没下下来：给一块底色 + 名字首字，别留一个坑
              <div className="preview-cover empty" aria-hidden="true">
                {detail.name.slice(0, 1)}
              </div>
            )}

            <h2 className="preview-name">{detail.name}</h2>

            <p className="preview-meta dim small">
              <span>{t("preview.downloads", { count: thousands(detail.downloads) })}</span>
              <span>
                {detail.creator
                  ? t("preview.creator", { name: detail.creator })
                  : t("preview.creatorUnknown")}
              </span>
              <span>
                {detail.date_updated > 0
                  ? t("preview.updated", { when: whenText(detail.date_updated * 1000) })
                  : t("preview.updatedUnknown")}
              </span>
            </p>

            {tags.length > 0 && (
              <div className="preview-tags">
                {tags.slice(0, TAG_LIMIT).map((tag) => (
                  <span className="chip" key={tag}>
                    {tag}
                  </span>
                ))}
                {extra > 0 && <span className="chip dim">{t("preview.more", { count: extra })}</span>}
              </div>
            )}

            {text && (
              <div className="preview-desc">
                <p className={expanded ? "preview-text open" : "preview-text"}>{text}</p>
                {longText && (
                  <button
                    className="btn ghost small"
                    onClick={() => setExpanded((v) => !v)}
                  >
                    {expanded ? t("preview.collapse") : t("preview.expand")}
                  </button>
                )}
              </div>
            )}

            {!text && <p className="dim small">作者还没有写简介。</p>}

            <div className="preview-actions">
              <button
                className="btn"
                disabled={installed || installing !== null}
                onClick={() =>
                  onInstall({
                    id: detail.id,
                    name: detail.name,
                    summary: detail.summary,
                    downloads: detail.downloads,
                  })
                }
              >
                {installed
                  ? t("preview.installed")
                  : installing === detail.id
                    ? t("action.installing")
                    : t("action.install")}
              </button>
            </div>
          </>
        )}
      </aside>
    </>
  );
}
