// 矿脉：找 mod、装 mod。装的时候后端会下载好、合并进游戏，装完直接能进游戏看到。
//
// 这一页只排版：搜与翻页在 `useSearch`，装（预检 + 排队）在 `useInstallFlow`，
// 列表长什么样在 `ModGrid`。
import ErrorHint from "../ErrorHint";
import BulkBar from "../components/BulkBar";
import ModGrid from "../components/ModGrid";
import Pager from "../components/Pager";
import PrecheckDialog from "../components/PrecheckDialog";
import QueueBar from "../components/QueueBar";
import SearchBar from "../components/SearchBar";
import StaleBanner from "../components/StaleBanner";
import { useBulkSelection } from "../hooks/useBulkSelection";
import { useInstallFlow } from "../hooks/useInstallFlow";
import { useSearch } from "../hooks/useSearch";
import { t, thousands } from "../i18n";
import { useAppStore } from "../store/appStore";

type Props = {
  onGoSettings: () => void;
};

export default function MineVein({ onGoSettings }: Props) {
  const s = useSearch();
  const page = s.page;
  const items = page?.items ?? [];
  // 勾选放全局：切到背包再切回来，勾的还是原来那几个
  const { page: here, selected: ids, setSelection } = useAppStore();
  const sel = useBulkSelection(
    items.map((m) => m.id),
    ids,
    setSelection,
    here === "mine",
  );
  // 一批跑完之后把勾选清掉，免得下次手一滑又装一遍同样的东西
  const flow = useInstallFlow(() => setSelection([]));

  return (
    <section>
      <h1>{t("mine.title")}</h1>
      <p className="dim">{t("mine.lead")}</p>

      <SearchBar
        value={s.draft}
        busy={s.busy}
        onChange={s.setDraft}
        onSearch={() => s.searchNow()}
      />

      {s.note && <p className="note ok">{s.note}</p>}
      {s.error && <ErrorHint error={s.error} onRetry={s.retry} onGoSettings={onGoSettings} />}

      {page?.stale && (
        <StaleBanner cachedAt={page.cached_at} busy={s.busy} onRetry={() => s.goPage(page.page)} />
      )}

      {page && items.length > 0 && (
        <>
          <p className="dim small">
            {page.query.trim()
              ? t("mine.count", {
                  query: page.query,
                  total: thousands(page.total),
                  page: page.page,
                })
              : t("mine.countHot", { total: thousands(page.total), page: page.page })}
          </p>
          <ModGrid
            items={items}
            installing={s.installing}
            onInstall={(mod) => flow.precheckThenInstall([mod], false)}
            selected={ids}
            onToggle={(id, shift) => sel.toggleWithShift(id, shift)}
          />
          <Pager
            page={page.page}
            hasPrev={page.page > 1}
            hasNext={items.length > 0}
            busy={s.busy}
            onPrev={() => s.goPage(page.page - 1)}
            onNext={() => s.goPage(page.page + 1)}
          />
        </>
      )}

      {page && items.length === 0 && <p className="dim">{t("mine.empty")}</p>}
      {!page && !s.busy && !s.error && <p className="dim">{t("mine.idle")}</p>}

      <BulkBar
        count={ids.length}
        busy={flow.queue.running}
        onInstall={() => flow.precheckThenInstall(items.filter((m) => ids.includes(m.id)), true)}
        onClear={sel.clear}
      />
      <QueueBar queue={flow.queue} />

      {flow.asking && (
        <PrecheckDialog
          report={flow.asking.report}
          count={flow.asking.list.length}
          onConfirm={flow.confirmInstall}
          onCancel={flow.cancelInstall}
        />
      )}
    </section>
  );
}
