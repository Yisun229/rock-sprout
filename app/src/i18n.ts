// 取文案的唯一入口：`t("nav.mine")`，带占位符的写 `t("bag.file", { name: "x.zip" })`。
import { zhCN, type TextKey } from "./locales/zh-CN";

export type { TextKey };

/** 按 key 取一条文案，把 `{name}` 这种占位符换成传进来的值。 */
export function t(key: TextKey, vars?: Record<string, string | number>): string {
  let text: string = zhCN[key];
  if (!vars) return text;
  for (const [name, value] of Object.entries(vars)) {
    text = text.replaceAll(`{${name}}`, String(value));
  }
  return text;
}

/** 数量加千分位（下载量那种）。 */
export function thousands(n: number): string {
  return n.toLocaleString("zh-CN");
}

/** 缓存时间（unix 毫秒）→「10 月 27 日 18:43」。取不到时间就说「上次」。 */
export function whenText(ms: number | null): string {
  if (!ms) return t("when.unknown");
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getMonth() + 1} 月 ${d.getDate()} 日 ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}
