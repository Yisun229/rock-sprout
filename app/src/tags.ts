/**
 * mod.io 的「类型」标签一共有 9 个真实名字（实测自 /games/2475/tags 的 Type 组）。
 * 筛选下拉和详情面板里的标签都从这里取中文名，别各写一份。
 */
export const TYPE_TAGS: [string, string][] = [
  ["QoL", "体验优化"],
  ["Framework", "前置框架"],
  ["Visual", "视觉"],
  ["Cosmetic", "外观"],
  ["Gameplay", "玩法"],
  ["Audio", "音效"],
  ["Tools", "工具"],
  ["Animation", "动画"],
  ["Auto-Verified", "官方认证"],
];

const LABELS = new Map(TYPE_TAGS);

/** 英文标签名 → 中文名。表里没有的（以后 mod.io 新加的）就原样显示，不至于空着。 */
export function tagLabel(name: string): string {
  return LABELS.get(name) ?? name;
}
