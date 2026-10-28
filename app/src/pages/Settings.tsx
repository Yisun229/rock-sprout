// 设置：令牌、配置搬运、现场备份、体检、诊断信息五张卡。
//
// 每张卡自己管自己的状态——它们之间没有共享的东西，拆开更好读。
import AuditCard from "../components/settings/AuditCard";
import ConfigCard from "../components/settings/ConfigCard";
import DiagnosticsCard from "../components/settings/DiagnosticsCard";
import SnapshotCard from "../components/settings/SnapshotCard";
import TokenCard from "../components/settings/TokenCard";
import { t } from "../i18n";

export default function Settings() {
  return (
    <section>
      <h1>{t("settings.title")}</h1>
      <p className="dim">{t("settings.lead")}</p>
      <TokenCard />
      <ConfigCard />
      <SnapshotCard />
      <AuditCard />
      <DiagnosticsCard />
    </section>
  );
}
