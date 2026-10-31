import { useEffect, useRef, useState } from "react";
import { call } from "../api";
import { t } from "../i18n";
import { showToast } from "../toast";

type Props = {
  /** 要交给系统浏览器打开的地址 */
  url: string;
  /** 按钮上平时显示的字，不传就显示地址本身 */
  label?: string;
  className?: string;
};

/**
 * 一个「长得像链接、其实是按钮」的东西。
 *
 * 为什么不直接用 `<a href>`：在这个窗口里点链接，WebView 会自己导航过去，
 * 整个界面被网页替换、没有返回入口。现在走 `open_url`，页面不动，浏览器另开。
 * 点了先把字换成「正在打开浏览器…」，底部出一条 2 秒自消的轻提示。
 */
export default function LinkButton({ url, label, className }: Props) {
  const [opening, setOpening] = useState(false);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => () => window.clearTimeout(timer.current), []);

  async function open() {
    setOpening(true);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setOpening(false), 1200);

    try {
      await call<string>("open_url", { url });
      showToast(t("open.opened"));
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      showToast(t("open.failed", { message }));
    }
  }

  return (
    <button
      type="button"
      className={className ? `link-btn ${className}` : "link-btn"}
      onClick={() => void open()}
      data-url={url}
    >
      {opening ? t("open.opening") : (label ?? url)}
    </button>
  );
}
