// 「今天」页的商店块：当前价、折扣、在线人数，外加一句 DLC 的汇总。
//
// 三件事分开降级：
// - 价格/人数来自 Steam，拉不到就用 10 分钟内的缓存，再没有就留空写一句；
// - DLC 那句来自仓库里的静态表（`app/src/data/dlc.json`），不联网、永远都在。
//
// 不按键逐个查 DLC：打开一次页面发 14 个请求不值当，价格过期写在表的 $comment 里。
import { useCallback, useEffect, useState } from "react";
import dlcFile from "../../data/dlc.json";
import { call, toApiError, type ApiError, type StoreSnapshot } from "../../api";
import ErrorHint from "../../ErrorHint";
import { t, whenText } from "../../i18n";

/** 分 → 「¥108.00」；不认识币种时退回「CNY 108.00」。 */
function money(cents: number, currency: string): string {
  const sign: Record<string, string> = { CNY: "¥", USD: "$", EUR: "€", GBP: "£", JPY: "¥" };
  const s = sign[currency];
  const amount = (cents / 100).toFixed(2);
  return s ? `${s}${amount}` : `${currency} ${amount}`;
}

/** DLC 汇总：条数与价格区间都从静态表算，不硬编码。 */
function dlcLine(): { count: number; lo: string; hi: string; currency: string } | null {
  const items = dlcFile.items.filter((x) => typeof x.price === "number" && x.price !== null);
  if (items.length === 0) return null;
  const cents = items.map((x) => x.price as number);
  const lo = Math.min(...cents);
  const hi = Math.max(...cents);
  const currency = items[0].currency;
  // 整数价就不带小数点（33–58 比 33.00–58.00 顺眼），有零头才保留两位
  const fmt = (c: number) => (c % 100 === 0 ? String(c / 100) : (c / 100).toFixed(2));
  return { count: items.length, lo: fmt(lo), hi: fmt(hi), currency };
}

export default function StoreBlock() {
  const [snap, setSnap] = useState<StoreSnapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);

  const load = useCallback(async (refresh: boolean) => {
    setBusy(true);
    try {
      setSnap(await call<StoreSnapshot>("store_snapshot", { refresh }));
      setError(null);
    } catch (e) {
      setError(toApiError(e));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void load(false);
  }, [load]);

  const price = snap?.price ?? null;
  const hasDiscount = !!price && price.available && price.discountPercent > 0 && price.initial > price.final;
  const dlc = dlcLine();
  const cnPrice = dlc?.currency === "CNY";

  return (
    <section className="card today-block" data-block="store">
      <div className="today-head">
        <h2 className="card-title">{t("today.store.title")}</h2>
        <button className="btn ghost small" disabled={busy} onClick={() => void load(true)}>
          {busy ? t("today.store.refreshing") : t("today.store.refresh")}
        </button>
      </div>

      {error && <ErrorHint error={error} />}

      {!snap && !error && <p className="dim small">{t("today.store.loading")}</p>}

      {snap && (
        <>
          <ul className="store-rows">
            <li className="store-row" data-field="price">
              <span className="store-label">{t("today.store.priceLabel")}</span>
              <span className="store-value mono">
                {price && price.available ? (
                  <>
                    <span data-price="final">{money(price.final, price.currency)}</span>
                    {hasDiscount && (
                      <>
                        {" "}
                        <span className="store-was" data-price="initial">
                          {money(price.initial, price.currency)}
                        </span>{" "}
                        <span className="store-off" data-price="discount">
                          {t("today.store.discount", { n: price.discountPercent })}
                        </span>
                      </>
                    )}
                  </>
                ) : (
                  t("today.store.noPrice")
                )}
              </span>
            </li>
            <li className="store-row" data-field="players">
              <span className="store-label">{t("today.store.playersLabel")}</span>
              <span className="store-value mono" data-players={snap.players ?? ""}>
                {snap.players === null
                  ? t("today.store.noPlayers")
                  : t("today.store.players", { n: snap.players.toLocaleString("en-US") })}
              </span>
            </li>
            {dlc && (
              <li className="store-row" data-field="dlc">
                <span className="store-label">{t("today.store.dlcLabel")}</span>
                <span className="store-value" data-dlc-count={dlc.count}>
                  {t("today.store.dlc", { count: dlc.count, lo: `${cnPrice ? "¥" : ""}${dlc.lo}`, hi: `${cnPrice ? "¥" : ""}${dlc.hi}` })}
                  {!cnPrice && <span className="dim">（{dlc.currency}）</span>}
                </span>
              </li>
            )}
          </ul>

          <p className="dim small">
            {snap.fromCache || snap.stale
              ? `${t("today.store.cached")}${whenText(snap.fetchedAt * 1000)}`
              : snap.fetchedAt > 0
                ? ` ${t("today.store.fetched", { when: whenText(snap.fetchedAt * 1000) })}`
                : ""}
          </p>
        </>
      )}
    </section>
  );
}
