
import root from '../root.js';
import { set_building, set_prerendering } from '__sveltekit/environment';
import { set_assets } from '$app/paths/internal/server';
import { set_manifest, set_read_implementation } from '__sveltekit/server';
import { set_private_env, set_public_env } from '../../../node_modules/@sveltejs/kit/src/runtime/shared-server.js';

export const options = {
	app_template_contains_nonce: false,
	async: false,
	csp: {"mode":"auto","directives":{"upgrade-insecure-requests":false,"block-all-mixed-content":false},"reportOnly":{"upgrade-insecure-requests":false,"block-all-mixed-content":false}},
	csrf_check_origin: true,
	csrf_trusted_origins: [],
	embedded: false,
	env_public_prefix: 'PUBLIC_',
	env_private_prefix: '',
	hash_routing: false,
	hooks: null, // added lazily, via `get_hooks`
	preload_strategy: "modulepreload",
	root,
	service_worker: false,
	service_worker_options: undefined,
	templates: {
		app: ({ head, body, assets, nonce, env }) => "<!doctype html>\r\n<html lang=\"en\">\r\n  <head>\r\n    <meta charset=\"utf-8\" />\r\n    <link rel=\"icon\" href=\"" + assets + "/favicon.png\" />\r\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\r\n    <title>ClearComms - Aviation Audio Control</title>\r\n    <style>\r\n      /* Global theme variables */\r\n      :root {\r\n        color-scheme: light dark;\r\n      }\r\n\r\n      html {\r\n        background: transparent;\r\n      }\r\n\r\n      /* Light mode (default) */\r\n      :root {\r\n        --bg-primary: #f5f5f5;\r\n        --bg-secondary: #ffffff;\r\n        --bg-card: rgba(255, 255, 255, 0.6);\r\n        --bg-card-hover: rgba(255, 255, 255, 0.8);\r\n        --bg-card-mapped: rgba(255, 255, 255, 0.8);\r\n        --text-primary: #1a1a1a;\r\n        --text-secondary: #666666;\r\n        --text-muted: #888888;\r\n        --border-color: rgba(0, 0, 0, 0.1);\r\n        --border-color-strong: rgba(0, 0, 0, 0.2);\r\n        --accent-primary: #24c8db;\r\n        --accent-secondary: #396cd8;\r\n        --status-success: #4ade80;\r\n        --status-error: #ff3e00;\r\n        --slider-bg: rgba(0, 0, 0, 0.1);\r\n        --button-bg: rgba(0, 0, 0, 0.05);\r\n        --button-hover: rgba(0, 0, 0, 0.1);\r\n      }\r\n\r\n      /* Dark mode */\r\n      @media (prefers-color-scheme: dark) {\r\n        :root {\r\n          --bg-primary: #0f0f0f;\r\n          --bg-secondary: #1a1a1a;\r\n          --bg-card: rgba(255, 255, 255, 0.05);\r\n          --bg-card-hover: rgba(255, 255, 255, 0.08);\r\n          --bg-card-mapped: rgba(255, 255, 255, 0.1);\r\n          --text-primary: #f6f6f6;\r\n          --text-secondary: #aaaaaa;\r\n          --text-muted: #666666;\r\n          --border-color: rgba(255, 255, 255, 0.1);\r\n          --border-color-strong: rgba(255, 255, 255, 0.2);\r\n          --accent-primary: #24c8db;\r\n          --accent-secondary: #396cd8;\r\n          --status-success: #4ade80;\r\n          --status-error: #ff3e00;\r\n          --slider-bg: rgba(255, 255, 255, 0.1);\r\n          --button-bg: rgba(255, 255, 255, 0.05);\r\n          --button-hover: rgba(255, 255, 255, 0.1);\r\n        }\r\n      }\r\n\r\n      body {\r\n        margin: 0;\r\n        padding: 0;\r\n        font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;\r\n        background: transparent;\r\n        color: var(--text-primary);\r\n        transition: color 0.2s ease;\r\n      }\r\n    </style>\r\n    " + head + "\r\n  </head>\r\n  <body data-sveltekit-preload-data=\"hover\">\r\n    <div style=\"display: contents\">" + body + "</div>\r\n  </body>\r\n</html>\r\n",
		error: ({ status, message }) => "<!doctype html>\n<html lang=\"en\">\n\t<head>\n\t\t<meta charset=\"utf-8\" />\n\t\t<title>" + message + "</title>\n\n\t\t<style>\n\t\t\tbody {\n\t\t\t\t--bg: white;\n\t\t\t\t--fg: #222;\n\t\t\t\t--divider: #ccc;\n\t\t\t\tbackground: var(--bg);\n\t\t\t\tcolor: var(--fg);\n\t\t\t\tfont-family:\n\t\t\t\t\tsystem-ui,\n\t\t\t\t\t-apple-system,\n\t\t\t\t\tBlinkMacSystemFont,\n\t\t\t\t\t'Segoe UI',\n\t\t\t\t\tRoboto,\n\t\t\t\t\tOxygen,\n\t\t\t\t\tUbuntu,\n\t\t\t\t\tCantarell,\n\t\t\t\t\t'Open Sans',\n\t\t\t\t\t'Helvetica Neue',\n\t\t\t\t\tsans-serif;\n\t\t\t\tdisplay: flex;\n\t\t\t\talign-items: center;\n\t\t\t\tjustify-content: center;\n\t\t\t\theight: 100vh;\n\t\t\t\tmargin: 0;\n\t\t\t}\n\n\t\t\t.error {\n\t\t\t\tdisplay: flex;\n\t\t\t\talign-items: center;\n\t\t\t\tmax-width: 32rem;\n\t\t\t\tmargin: 0 1rem;\n\t\t\t}\n\n\t\t\t.status {\n\t\t\t\tfont-weight: 200;\n\t\t\t\tfont-size: 3rem;\n\t\t\t\tline-height: 1;\n\t\t\t\tposition: relative;\n\t\t\t\ttop: -0.05rem;\n\t\t\t}\n\n\t\t\t.message {\n\t\t\t\tborder-left: 1px solid var(--divider);\n\t\t\t\tpadding: 0 0 0 1rem;\n\t\t\t\tmargin: 0 0 0 1rem;\n\t\t\t\tmin-height: 2.5rem;\n\t\t\t\tdisplay: flex;\n\t\t\t\talign-items: center;\n\t\t\t}\n\n\t\t\t.message h1 {\n\t\t\t\tfont-weight: 400;\n\t\t\t\tfont-size: 1em;\n\t\t\t\tmargin: 0;\n\t\t\t}\n\n\t\t\t@media (prefers-color-scheme: dark) {\n\t\t\t\tbody {\n\t\t\t\t\t--bg: #222;\n\t\t\t\t\t--fg: #ddd;\n\t\t\t\t\t--divider: #666;\n\t\t\t\t}\n\t\t\t}\n\t\t</style>\n\t</head>\n\t<body>\n\t\t<div class=\"error\">\n\t\t\t<span class=\"status\">" + status + "</span>\n\t\t\t<div class=\"message\">\n\t\t\t\t<h1>" + message + "</h1>\n\t\t\t</div>\n\t\t</div>\n\t</body>\n</html>\n"
	},
	version_hash: "1j6omsa"
};

export async function get_hooks() {
	let handle;
	let handleFetch;
	let handleError;
	let handleValidationError;
	let init;
	

	let reroute;
	let transport;
	

	return {
		handle,
		handleFetch,
		handleError,
		handleValidationError,
		init,
		reroute,
		transport
	};
}

export { set_assets, set_building, set_manifest, set_prerendering, set_private_env, set_public_env, set_read_implementation };
