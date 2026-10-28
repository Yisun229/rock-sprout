// 「勾几个装上」这条流程：先预检、再排队真装。
//
// 单独抽出来是因为矿脉页还要排版搜索结果，而这里的判断（什么时候该弹框、
// 预检自己出错要不要继续装）跟排版无关，混在一起两边都看不清。
import { useState } from "react";
import { call, type ModSummary, type PrecheckReport } from "../api";
import { useQueue } from "./useQueue";

export type InstallFlow = {
  queue: ReturnType<typeof useQueue>;
  /** 正在等用户点「就这么装」的那批 */
  asking: { report: PrecheckReport; list: ModSummary[] } | null;
  /** 预检一下再装；`alwaysAsk` 为真时不管撞不撞都先弹框 */
  precheckThenInstall: (list: ModSummary[], alwaysAsk: boolean) => void;
  confirmInstall: () => void;
  cancelInstall: () => void;
};

export function useInstallFlow(onFinished: () => void): InstallFlow {
  const queue = useQueue(onFinished);
  const [asking, setAsking] = useState<{ report: PrecheckReport; list: ModSummary[] } | null>(null);

  const start = (list: ModSummary[]) => {
    queue.start(list.map((m) => ({ kind: "install" as const, id: m.id, name: m.name })));
  };

  const precheckThenInstall = (list: ModSummary[], alwaysAsk: boolean) => {
    if (list.length === 0) return;
    void (async () => {
      try {
        const report = await call<PrecheckReport>("precheck_install", {
          modIds: list.map((m) => String(m.id)),
        });
        // 一次装好几个时一定把结果摆出来；单个装只在真撞了的时候才拦
        if (alwaysAsk || report.conflicts.length > 0) {
          setAsking({ report, list });
          return;
        }
      } catch {
        // 预检自己出错不该拦住安装；真装不上，原因会在队列里写清楚
      }
      start(list);
    })();
  };

  const confirmInstall = () => {
    if (asking) start(asking.list);
    setAsking(null);
  };

  return {
    queue,
    asking,
    precheckThenInstall,
    confirmInstall,
    cancelInstall: () => setAsking(null),
  };
}
