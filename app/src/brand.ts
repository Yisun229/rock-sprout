// 品牌名的唯一来源：产品名从 tauri.conf.json 读进来（见 vite.config.ts 的 define），
// 中文名在这里写一次。界面上凡是出现「岩笋」「产品名」的地方都用这两个常量，
// 不要在别处再写死字符串——改名时只改 tauri.conf.json 一处。
declare const __PRODUCT_NAME__: string;

/** 英文产品名，跟安装目录名、exe 名、窗口标题同源 */
export const PRODUCT = __PRODUCT_NAME__;
/** 中文名（品牌固定，不跟着产品名变） */
export const BRAND_ZH = "岩笋";
/** 完整标题：窗口、文档标题、关于弹层都用它 */
export const APP_TITLE = `${BRAND_ZH} · ${PRODUCT}`;
