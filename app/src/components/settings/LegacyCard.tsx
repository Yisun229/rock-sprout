// 认出旧版本留下的数据 —— 只在真扫到东西时才出现在设置页顶上。
//
// 扫不到、或者用户已经选过（沿用 / 不用），这块就完全不渲染。
import { useRef, useState } from "react";

import { call, toApiError, type LegacyCandidate } from "../../api";
import { t } from "../../i18n";

export default function LegacyCard() {
  const [cand, setCand] = useState<LegacyCandidate | null>(null);
  const [done, setDone] = useState<string>("");
  const [note, setNote] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const started = useRef(false);

  // 只在第一次挂载时扫一次；三页常驻，所以不会因为切页反复扫。
  if (!started.current) {
    started.current = true;
    void (async () => {
      try {
        const found = await call<LegacyCandidate | null>("legacy_data");
        if (found) setCand(found);
      } catch {
        // 扫不到就当没有，不打扰用户
      }
    })();
  }

  if (!cand || done) {
    // 处理完了只留一句结果（沿用成功要说清复制了多少）
    return done ? (
      <div className="card">
        <div className="card-title">{t("legacy.title")}</div>
        <p className="note ok">{done}</p>
      </div>
    ) : null;
  }

  async function decide(adopt: boolean) {
    if (!cand) return;
    setBusy(true);
    setNote("");
    try {
      const message = await call<string>("resolve_legacy", { path: cand.path, adopt });
      setDone(message);
    } catch (e) {
      setNote(toApiError(e).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="card">
      <div className="card-title">{t("legacy.title")}</div>
      <p className="note">{t("legacy.hint")}</p>
      <p className="note small dim">
        {t("legacy.found", {
          path: cand.path,
          mods: cand.mods,
          snapshots: cand.snapshots,
        })}
      </p>
      <p className="note small dim">{t("legacy.safe")}</p>
      {note && <p className="note small dim">{note}</p>}
      <div className="searchbar">
        <button className="btn small" disabled={busy} onClick={() => void decide(true)}>
          {busy ? t("action.working") : t("legacy.adopt")}
        </button>
        <button className="btn ghost small" disabled={busy} onClick={() => void decide(false)}>
          {t("legacy.decline")}
        </button>
      </div>
    </div>
  );
}
