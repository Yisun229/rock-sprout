// 设置页的商店地区卡：查 Steam 价格时用哪个国家/地区的价。
//
// 只影响商店块显示的是哪个区的价格（默认 CN）。写错了不会坏东西——Steam 查回来
// 没有价格时那块写「这个区看不到价格」，而不是报错。
import { useCallback, useEffect, useState } from "react";
import ErrorHint from "../../ErrorHint";
import { call, toApiError, type ApiError, type StoreSettings } from "../../api";
import { t } from "../../i18n";
import { useAppStore } from "../../store/appStore";

export default function StoreRegionCard() {
  const [country, setCountry] = useState("");
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const { page } = useAppStore();

  const refresh = useCallback(async () => {
    try {
      const s = await call<StoreSettings>("get_store_settings");
      setCountry(s.country);
      setDraft(s.country);
      setError(null);
    } catch (e) {
      setError(toApiError(e));
    }
  }, []);

  useEffect(() => {
    if (page !== "settings") return;
    void refresh();
  }, [page, refresh]);

  async function save() {
    setBusy(true);
    setError(null);
    setSaved(false);
    try {
      const s = await call<StoreSettings>("set_store_country", { country: draft });
      setCountry(s.country);
      setDraft(s.country);
      setSaved(true);
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <div className="card">
        <div className="card-title">{t("store.region.title")}</div>
        <p className="dim small">{t("store.region.hint")}</p>
        <div className="searchbar">
          <input
            className="input mono store-region"
            value={draft}
            maxLength={2}
            spellCheck={false}
            aria-label={t("store.region.label")}
            onChange={(e) => {
              setDraft(e.target.value.toUpperCase());
              setSaved(false);
            }}
          />
          <button className="btn" disabled={busy || draft === country} onClick={() => void save()}>
            {t("store.region.save")}
          </button>
        </div>
        <p className="dim small">{t("store.region.current", { country })}</p>
      </div>
      {saved && <p className="note ok">{t("store.region.saved")}</p>}
      {error && <ErrorHint error={error} />}
    </>
  );
}
