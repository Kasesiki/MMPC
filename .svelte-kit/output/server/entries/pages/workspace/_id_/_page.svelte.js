import { e as escape_html } from "../../../../chunks/escaping.js";
import "clsx";
import "@tauri-apps/api/event";
import "@tauri-apps/api/core";
import "@sveltejs/kit/internal";
import "../../../../chunks/exports.js";
import "../../../../chunks/utils.js";
import "@sveltejs/kit/internal/server";
import "../../../../chunks/root.js";
import "../../../../chunks/state.svelte.js";
import "../../../../chunks/workspace.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    let { params } = $$props;
    $$renderer2.push(`<div class="p-4 lg:p-6"><div class="flex items-center gap-3 mb-4"><button class="btn btn-ghost btn-sm btn-circle" aria-label="返回"><svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M19 12H5M12 19l-7-7 7-7"></path></svg></button> <div><h1 class="text-2xl font-bold">${escape_html("工作区")}</h1> <p class="text-sm text-base-content/60">${escape_html("")}</p></div></div> `);
    {
      $$renderer2.push("<!--[-1-->");
    }
    $$renderer2.push(`<!--]--> `);
    {
      $$renderer2.push("<!--[-1-->");
      $$renderer2.push(`<div class="alert alert-warning">工作区未找到</div>`);
    }
    $$renderer2.push(`<!--]--></div>`);
  });
}
export {
  _page as default
};
