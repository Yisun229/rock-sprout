// 存档选择器：多份才给下拉，一份就静态一行。
//
// 「我的数据」和「本周」共用同一个——两页看到的是同一份存档，
// 换档也走同一个 `set_save_file`，别让它们各自记一个选择。
import type { SaveCandidate } from "../api";
import { t, whenText } from "../i18n";

/** 选择器里那一行字：「…191195 · 141 局 · 改于 11 月 3 日 21:05」 */
export function candidateText(item: SaveCandidate, when: string): string {
  return item.games < 0
    ? t("data.candidateBroken", { account: item.account, when })
    : t("data.candidate", { account: item.account, games: item.games, when });
}

export function SavePicker({
  items,
  busy,
  onPick,
}: {
  items: SaveCandidate[];
  busy?: boolean;
  onPick: (id: string) => void;
}) {
  const current = items.find((it) => it.active) ?? null;

  if (items.length > 1) {
    return (
      <label className="save-pick">
        <span className="dim small">{t("data.savePick")}</span>
        <select value={current?.id ?? ""} disabled={busy} onChange={(e) => onPick(e.target.value)}>
          {items.map((it) => (
            <option key={it.id} value={it.id}>
              {candidateText(it, whenText(it.modified_ms))}
            </option>
          ))}
        </select>
        {busy ? <span className="dim small">{t("data.switching")}</span> : null}
      </label>
    );
  }

  if (!current) return null;
  return <span className="dim small save-only">{candidateText(current, whenText(current.modified_ms))}</span>;
}
