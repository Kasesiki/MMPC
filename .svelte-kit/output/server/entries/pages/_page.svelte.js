import "clsx";
import "../../chunks/workspace.js";
import "@sveltejs/kit/internal";
import "../../chunks/exports.js";
import "../../chunks/utils.js";
import "@sveltejs/kit/internal/server";
import "../../chunks/root.js";
import "../../chunks/state.svelte.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    $$renderer2.push(`<div class="min-h-screen p-6"><div class="flex items-center justify-between mb-8"><div><h1 class="text-3xl font-bold">MMPC</h1> <p class="text-base-content/60 text-sm mt-1">Minecraft Modpack Maker</p></div> <button class="btn btn-primary">+ 新建工作区</button></div> `);
    {
      $$renderer2.push("<!--[0-->");
      $$renderer2.push(`<div class="flex justify-center py-24"><span class="loading loading-spinner loading-lg"></span></div>`);
    }
    $$renderer2.push(`<!--]--></div> `);
    {
      $$renderer2.push("<!--[-1-->");
    }
    $$renderer2.push(`<!--]-->`);
  });
}
export {
  _page as default
};
