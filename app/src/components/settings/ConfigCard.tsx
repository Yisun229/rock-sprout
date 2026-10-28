// 设置页的第二张卡：把「装了哪些 mod」导成 json，或者读一份别人的来比对。
import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import ErrorHint from "../../ErrorHint";
import {
  call,
  toApiError,
  type ApiError,
  type ConfigImportOutcome,
  type ExportOutcome,
} from "../../api";
import { t } from "../../i18n";

export default function ConfigCard() {
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState<ApiError | null>(null);

  async function exportConfig() {
    setBusy(true);
    setError(null);
    setNote("");
    try {
      const res = await call<ExportOutcome>("export_config", { dir: null });
      setNote(t("settings.exported", { path: res.path, count: res.count }));
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy(false);
    }
  }

  async function importConfig() {
    const picked = await open({
      multiple: false,
      filters: [{ name: t("settings.configFilter"), extensions: ["json"] }],
    });
    if (!picked || Array.isArray(picked)) return;

    setBusy(true);
    setError(null);
    setNote("");
    try {
      const res = await call<ConfigImportOutcome>("import_config", { path: picked });
      setNote(t("settings.imported", { summary: res.summary, version: res.app_version }));
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <div className="card">
        <div className="card-title">{t("settings.config")}</div>
        <p className="dim small">{t("settings.configHint")}</p>
        <div className="searchbar">
          <button className="btn" disabled={busy} onClick={() => void exportConfig()}>
            {t("action.exportConfig")}
          </button>
          <button className="btn ghost" disabled={busy} onClick={() => void importConfig()}>
            {t("action.importConfig")}
          </button>
        </div>
      </div>

      {note && <p className="note ok">{note}</p>}
      {error && <ErrorHint error={error} />}
    </>
  );
}
