// 存档里记的职业代号 → 界面上的中文。
//
// 装备页和「我的数据」都要用；认不出（代号缺失或者是新职业）就返回 null，
// 由调用方决定说「认不出是哪个职业」还是「角色 N」。
import { t } from "./i18n";

const CLASS_TEXT = {
  driller: "equip.classDriller",
  engineer: "equip.classEngineer",
  gunner: "equip.classGunner",
  scout: "equip.classScout",
} as const;

export function classLabel(code: string | null | undefined): string | null {
  if (!code) return null;
  const key = CLASS_TEXT[code as keyof typeof CLASS_TEXT];
  return key ? t(key) : null;
}
