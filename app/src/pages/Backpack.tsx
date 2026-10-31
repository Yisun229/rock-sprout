// 背包：已经装进游戏的 mod。卸载后后端会立刻重建合并包。
// 「导入」是给不走 mod.io 的本地 mod 用的：选一个 pak/zip，或者选一个装着 pak 的文件夹。
//
// 数据和动作都在 `useInstalled` 里，这一页只排版；批量卸走底部队列，
// 卸之前先弹一层把所有要卸的名字列出来——这个动作不可逆，值得多问一句。
import { useEffect, useRef, useState } from "react";
import type { InstalledMod } from "../api";
import BagToolbar from "../components/BagToolbar";
import BulkBar from "../components/BulkBar";
import ConfirmDialog from "../components/ConfirmDialog";
import ConflictDrawer from "../components/ConflictDrawer";
import ModRow from "../components/ModRow";
import NameConflictDialog from "../components/NameConflictDialog";
import QueueBar from "../components/QueueBar";
import EmptyState from "../components/states/EmptyState";
import ErrorState from "../components/states/ErrorState";
import SkeletonRows from "../components/states/SkeletonRows";
import { useBulkSelection } from "../hooks/useBulkSelection";
import { useInstalled } from "../hooks/useInstalled";
import { useQueue } from "../hooks/useQueue";
import { t } from "../i18n";
import { useAppStore } from "../store/appStore";

type Props = {
  /** 空了的时候给个去处：直接切去矿脉 */
  onGoMine: () => void;
};

export default function Backpack({ onGoMine }: Props) {
  const { page, selected, setSelection } = useAppStore();
  const bag = useInstalled();
  const items = bag.items ?? [];
  const queue = useQueue(() => void bag.refresh());
  const sel = useBulkSelection(
    items.map((m) => m.id),
    selected,
    setSelection,
    page === "bag",
  );
  const [onlyConflicts, setOnlyConflicts] = useState(false);
  const [drawer, setDrawer] = useState(false);
  const [asking, setAsking] = useState<InstalledMod[] | null>(null);


  const scan = new Map((bag.conflicts?.mods ?? []).map((m) => [m.id, m]));
  const hasConflict = (m: InstalledMod) => {
    const s = scan.get(m.id);
    return !!s && (s.overridden > 0 || s.covers > 0);
  };
  const shown = onlyConflicts ? items.filter(hasConflict) : items;
  const picked = items.filter((m) => selected.includes(m.id));

  // 三个页都常驻了，所以每次切回背包要重新读一遍：装完 mod 之后回到这儿得看得见。
  const wasHere = useRef(false);
  useEffect(() => {
    const here = page === "bag";
    if (here && !wasHere.current) void bag.refresh();
    wasHere.current = here;
  }, [page, bag]);

  const doRemove = () => {
    const list = asking ?? [];
    setAsking(null);
    queue.start(list.map((m) => ({ kind: "remove" as const, id: m.id, name: m.name })));
    setSelection([]);
  };

  return (
    <section>
      <h1>{t("bag.title")}</h1>
      <p className="dim">{t("bag.lead")}</p>

      <BagToolbar
        busy={bag.busy}
        hasConflicts={(bag.conflicts?.total_paths ?? 0) > 0}
        onlyConflicts={onlyConflicts}
        onRefresh={() => void bag.refresh()}
        onPickFiles={() => void bag.pickFiles()}
        onPickFolder={() => void bag.pickFolder()}
        onOpenDrawer={() => setDrawer(true)}
        onToggleOnlyConflicts={setOnlyConflicts}
      />
      <p className="dim small">{t("bag.importHint")}</p>

      {bag.note && <p className="note ok">{bag.note}</p>}
      {bag.error && <ErrorState error={bag.error} onRetry={() => void bag.refresh()} />}
      {!bag.items && bag.busy && !bag.error && <SkeletonRows />}
      {bag.items && items.length === 0 && (
        <EmptyState
          title={t("bag.empty")}
          actionLabel={t("bag.emptyAction")}
          onAction={onGoMine}
        />
      )}

      {shown.length > 0 && (
        <ul className="list">
          {shown.map((m) => (
            <ModRow
              key={m.id}
              mod={m}
              busy={bag.busy}
              overridden={scan.get(m.id)?.overridden ?? 0}
              covers={scan.get(m.id)?.covers ?? 0}
              onRemove={() => void bag.remove(m)}
              onDropRecord={() => void bag.dropRecord(m)}
              selected={selected.includes(m.id)}
              onToggle={(shift) => sel.toggleWithShift(m.id, shift)}
            />
          ))}
        </ul>
      )}

      <BulkBar
        count={picked.length}
        busy={queue.running}
        onRemove={() => picked.length > 0 && setAsking(picked)}
        onClear={sel.clear}
      />
      <QueueBar queue={queue} />

      {drawer && <ConflictDrawer report={bag.conflicts} onClose={() => setDrawer(false)} />}

      {bag.conflict && (
        <NameConflictDialog
          message={bag.conflict.message}
          defaultName={bag.conflict.name}
          busy={bag.busy}
          onRename={(name) => void bag.importAs(bag.conflict!.path, name)}
          onReplace={() => void bag.importReplace(bag.conflict!.path)}
          onCancel={bag.clearConflict}
        />
      )}

      {asking && (
        <ConfirmDialog
          title={t("bag.confirmRemoveTitle", { count: asking.length })}
          lines={asking.map((m) => m.name)}
          confirmLabel={t("bulk.remove")}
          cancelLabel={t("conflict.precheckCancel")}
          onConfirm={doRemove}
          onCancel={() => setAsking(null)}
        />
      )}
    </section>
  );
}
