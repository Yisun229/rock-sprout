// 矿脉页的搜索框：一个输入框 + 一个按钮，回车等同点按钮。
import { t } from "../i18n";

type Props = {
  value: string;
  busy: boolean;
  onChange: (next: string) => void;
  onSearch: () => void;
};

export default function SearchBar({ value, busy, onChange, onSearch }: Props) {
  return (
    <div className="searchbar">
      <input
        className="input"
        placeholder={t("mine.placeholder")}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !busy) onSearch();
        }}
      />
      <button className="btn" disabled={busy} onClick={onSearch}>
        {busy ? t("action.searching") : t("action.search")}
      </button>
    </div>
  );
}
