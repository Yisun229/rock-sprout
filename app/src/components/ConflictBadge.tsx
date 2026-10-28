// 冲突徽章：一个 mod 跟同装的其他 mod 改到同一批游戏文件时，在它旁边挂个标。
//
// 两种情形分开说，因为要做的事不一样：
// - 「被覆盖」：这个 mod 有一部分内容被排后面的 mod 盖掉了，用户看到的效果会和它本来不一样；
// - 「覆盖了 N 处」：这个 mod 把别人的东西盖掉了，卸了它别人就能看出来。
// 两种都没有就不挂任何东西——不制造「好像一切正常」的假徽章。
import { t } from "../i18n";

type Props = {
  /** 有几个文件被别人盖住了 */
  overridden: number;
  /** 有几个文件盖住了别人 */
  covers: number;
};

export default function ConflictBadge({ overridden, covers }: Props) {
  if (overridden > 0) {
    return (
      <span className="badge bad" title={t("conflict.overridden", { count: overridden })}>
        {t("conflict.overriddenShort")}
      </span>
    );
  }
  if (covers > 0) {
    return (
      <span className="badge warn" title={t("conflict.covers", { count: covers })}>
        {t("conflict.coversShort", { count: covers })}
      </span>
    );
  }
  return null;
}
