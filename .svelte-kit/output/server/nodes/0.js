

export const index = 0;
let component_cache;
export const component = async () => component_cache ??= (await import('../entries/pages/_layout.svelte.js')).default;
export const universal = {
  "ssr": false
};
export const universal_id = "src/routes/+layout.ts";
export const imports = ["_app/immutable/nodes/0.BAO4GtCd.js","_app/immutable/chunks/zxVJSkaM.js","_app/immutable/chunks/D07b79_B.js","_app/immutable/chunks/7PdtRfbG.js"];
export const stylesheets = ["_app/immutable/assets/0.BvDuigeJ.css"];
export const fonts = [];
