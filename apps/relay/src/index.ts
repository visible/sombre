import { hex, network, room } from "./log";
import type { Env } from "./room";
import { GONE, refused } from "./room";

export { Gate } from "./gate";
export { Room } from "./room";

const SITE = "https://sombre.ai";

const guarded = (response: Response, url: URL) => {
  if (response.webSocket) {
    return response;
  }
  const socket = `${url.protocol === "https:" ? "wss" : "ws"}://${url.host}`;
  const headers = new Headers(response.headers);
  headers.set("Cache-Control", "no-store");
  headers.set("X-Content-Type-Options", "nosniff");
  headers.set("Referrer-Policy", "no-referrer");
  if (url.protocol === "https:") {
    headers.set("Strict-Transport-Security", "max-age=31536000");
  }
  headers.set(
    "Content-Security-Policy",
    `default-src 'self'; script-src 'self'; style-src 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ${socket}; frame-ancestors 'none'`
  );
  return new Response(response.body, { headers, status: response.status });
};

const text = (body: string, status: number) =>
  new Response(body, {
    headers: { "Content-Type": "text/plain; charset=utf-8" },
    status,
  });

const page = async (env: Env, url: URL) => {
  const asset = await env.ASSETS.fetch(new URL("/phone.html", url));
  const html = await asset.text();
  return new Response(
    html.replace(
      "<head>",
      '<head>\n    <meta name="relay" content="socket" />'
    ),
    { headers: { "Content-Type": "text/html; charset=utf-8" } }
  );
};

const pages = new Set(["/home.js", "/seen.js"]);

const scripts: Record<string, string> = {
  "/install": "/install.sh",
  "/install.ps1": "/install.ps1",
  "/install.sh": "/install.sh",
};

const plain = async (env: Env, url: URL, name: string) => {
  const asset = await env.ASSETS.fetch(new URL(name, url));
  return new Response(asset.body, {
    headers: { "Content-Type": "text/plain; charset=utf-8" },
    status: asset.status,
  });
};

const create = async (request: Request, env: Env) => {
  const address = network(request.headers.get("CF-Connecting-IP") ?? "");
  const admitted = await env.GATES.get(env.GATES.idFromName(address)).admit(
    Date.now()
  );
  if (!admitted) {
    return text(
      "too many new pairings from this network. wait a few minutes and try again",
      429
    );
  }
  const id = hex(12);
  const secret = hex(24);
  await env.ROOMS.get(env.ROOMS.idFromName(id)).init(secret);
  return Response.json({ room: id, secret });
};

const answer = (request: Request, env: Env, url: URL) => {
  if (request.method === "POST" && url.pathname === "/rooms") {
    return create(request, env);
  }
  if (request.method === "GET" && url.pathname === "/app.js") {
    return env.ASSETS.fetch(new URL("/app.js", url));
  }
  if (request.method === "GET" && url.pathname === "/") {
    return env.ASSETS.fetch(new URL("/index.html", url));
  }
  if (request.method === "GET" && pages.has(url.pathname)) {
    return env.ASSETS.fetch(new URL(url.pathname, url));
  }
  const script = scripts[url.pathname];
  if (request.method === "GET" && script) {
    return plain(env, url, script);
  }
  const found = room(url.pathname);
  if (!found) {
    return request.headers.get("Upgrade")?.toLowerCase() === "websocket"
      ? refused(4404, GONE)
      : Response.redirect(`${SITE}${url.pathname}${url.search}`, 308);
  }
  if (request.method === "GET" && found.action === "") {
    return page(env, url);
  }
  return env.ROOMS.get(env.ROOMS.idFromName(found.id)).fetch(request);
};

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    return guarded(await answer(request, env, url), url);
  },
} satisfies ExportedHandler<Env>;
