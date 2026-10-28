// 数据目录在哪里 —— 只有在「程序目录写不了、退到用户目录」时才显示。
//
// 正常情况下数据就在程序旁边，用户不需要知道这件事，所以这块什么都不占地方；
// 一旦退到了用户目录，就得说清楚在哪儿、为什么，还能一键打开那个文件夹。
import { useEffect, useRef, useState } from "react";

import { call, toApiError, type DataLocation } from "../../api";
import { t } from "../../i18n";

export default function DataLocationCard() {
  const [loc, setLoc] = useState<DataLocation | null>(null);
  const [note, setNote] = useState<string>("");
  const started = useRef(false);

  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void (async () => {
      try {
        setLoc(await call<DataLocation>("data_location"));
      } catch (e) {
        setNote(toApiError(e).message);
      }
    })();
  }, []);

  async function open() {
    try {
      await call<string>("open_data_dir");
      setNote("");
    } catch (e) {
      setNote(toApiError(e).message);
    }
  }

  // 只有退出便携模式时才提示；其余情况这块完全不出现
  if (!loc?.fell_back) return null;

  return (
    <div className="card">
      <div className="card-title">{t("data.fallbackTitle")}</div>
      <p className="note">{t("data.fallbackHint", { path: loc.path })}</p>
      {note && <p className="note small dim">{note}</p>}
      <div className="searchbar">
        <button className="btn small ghost" onClick={() => void open()}>
          {t("action.openFolder")}
        </button>
      </div>
    </div>
  );
}
