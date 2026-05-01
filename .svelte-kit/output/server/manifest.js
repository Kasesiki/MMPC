export const manifest = (() => {
function __memo(fn) {
	let value;
	return () => value ??= (value = fn());
}

return {
	appDir: "_app",
	appPath: "_app",
	assets: new Set(["favicon.png","svelte.svg","tauri.svg","vite.svg"]),
	mimeTypes: {".png":"image/png",".svg":"image/svg+xml"},
	_: {
		client: {start:"_app/immutable/entry/start.azNVOAAd.js",app:"_app/immutable/entry/app.CvdmAzrH.js",imports:["_app/immutable/entry/start.azNVOAAd.js","_app/immutable/chunks/Cwezdqkh.js","_app/immutable/chunks/D07b79_B.js","_app/immutable/chunks/D9uCMLKW.js","_app/immutable/entry/app.CvdmAzrH.js","_app/immutable/chunks/D07b79_B.js","_app/immutable/chunks/zxVJSkaM.js","_app/immutable/chunks/D9uCMLKW.js","_app/immutable/chunks/DNDr-6d5.js","_app/immutable/chunks/7PdtRfbG.js","_app/immutable/chunks/DsmvTmfY.js"],stylesheets:[],fonts:[],uses_env_dynamic_public:false},
		nodes: [
			__memo(() => import('./nodes/0.js')),
			__memo(() => import('./nodes/1.js')),
			__memo(() => import('./nodes/2.js')),
			__memo(() => import('./nodes/3.js'))
		],
		remotes: {
			
		},
		routes: [
			{
				id: "/",
				pattern: /^\/$/,
				params: [],
				page: { layouts: [0,], errors: [1,], leaf: 2 },
				endpoint: null
			},
			{
				id: "/workspace/[id]",
				pattern: /^\/workspace\/([^/]+?)\/?$/,
				params: [{"name":"id","optional":false,"rest":false,"chained":false}],
				page: { layouts: [0,], errors: [1,], leaf: 3 },
				endpoint: null
			}
		],
		prerendered_routes: new Set([]),
		matchers: async () => {
			
			return {  };
		},
		server_assets: {}
	}
}
})();
