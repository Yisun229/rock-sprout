// 设置页的第三张卡：一键把「这台机器什么情况」整理成几行，方便贴给别人看。
import { useState } from "react";
import ErrorHint from "../../ErrorHint";
import { call, toApiError, type ApiError } from "../../api";
import { writeClipboard } from "../../clipboard";
import { t } from "../../i18n";

export default function DiagnosticsCard() {
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [diag, setDiag] = useState("");
  const [error, setError] = useState<ApiError | null>(null);

  async function copyDiagnostics() {
    setBusy(true);
    setError(null);
    setNote("");
    try {
      const text = await call<string>("diagnostics");
      setDiag(text);
      setNote(
        (await writeClipboard(text)) ? t("settings.diagCopied") : t("settings.diagNotCopied"),
      );
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <div className="card">
        <div className="card-title">{t("settings.diag")}</div>
        <p className="dim small">{t("settings.diagHint")}</p>
        <div className="searchbar">
          <button className="btn" disabled={busy} onClick={() => void copyDiagnostics()}>
            {t("action.copyDiagnostics")}
          </button>
        </div>
        {diag && <pre className="diag">{diag}</pre>}
      </div>

      {note && <p className="note ok">{note}</p>}
      {error && <ErrorHint error={error} />}
    </>
  );
}
