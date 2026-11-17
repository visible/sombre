import { DurableObject } from "cloudflare:workers";

import type { Gate } from "./gate";
import type { Device } from "./log";
import {
  KEPT,
  append,
  over,
  cursor,
  frame,
  hex,
  label,
  machines,
  pack,
  rebuild,
  since,
  strings,
} from "./log";

export interface Env {
  ASSETS: Fetcher;
  GATES: DurableObjectNamespace<Gate>;
  ROOMS: DurableObjectNamespace<Room>;
}

const FLUSH = 2000;
const HOUR = 60 * 60 * 1000;
const IDLE = 30 * 24 * HOUR;
const SEEN = 64;
const ALIVE = 70_000;
const TOKENS = 16;
const BODY = 6 * 1024 * 1024;

export const GONE =
  "this chat isn't paired anymore. run /pair on your mac for a new link";
const UNPAIRED =
  "this phone isn't paired anymore. run /pair on your mac for a new link";
const USED =
  "this pairing link was already used. run /pair on your mac for a new one";

const text = (body: string, status = 200) =>
  new Response(body, {
    headers: { "Content-Type": "text/plain; charset=utf-8" },
    status,
  });

export const refused = (code: number, reason: string) => {
  const pair = new WebSocketPair();
  pair[1].accept();
  pair[1].close(code, reason);
  return new Response(null, { status: 101, webSocket: pair[0] });
};

const encoder = new TextEncoder();

const same = (one: string, two: string) => {
  const left = encoder.encode(one);
  const right = encoder.encode(two);
  return (
    left.byteLength === right.byteLength &&
    crypto.subtle.timingSafeEqual(left, right)
  );
};

const tell = (sockets: WebSocket[], body: string) => {
  for (const socket of sockets) {
    if (socket.readyState === WebSocket.OPEN) {
      socket.send(body);
    }
  }
};

export class Room extends DurableObject<Env> {
  private secret = "";
  private epoch = "";
  private devices: Device[] = [];
  private tokens: string[] = [];
  private flushed = 0;
  private pending: string[] = [];
  private received = 0;
  private read = 0;
  private seen: string[] = [];
  private touched = 0;
  private due = false;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    ctx.setWebSocketAutoResponse(
      new WebSocketRequestResponsePair("ping", "pong")
    );
    const { sql } = ctx.storage;
    const made = sql
      .exec(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'meta'"
      )
      .toArray();
    if (made.length > 0) {
      this.load();
    }
  }

  private tables() {
    const { sql } = this.ctx.storage;
    sql.exec(
      "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)"
    );
    sql.exec(
      "CREATE TABLE IF NOT EXISTS out (first INTEGER PRIMARY KEY, last INTEGER NOT NULL, items TEXT NOT NULL)"
    );
    sql.exec(
      "CREATE TABLE IF NOT EXISTS inbox (seq INTEGER PRIMARY KEY, id TEXT NOT NULL, item TEXT NOT NULL)"
    );
  }

  private load() {
    const { sql } = this.ctx.storage;
    const meta = new Map(
      sql
        .exec<{ key: string; value: string }>("SELECT key, value FROM meta")
        .toArray()
        .map((row) => [row.key, row.value])
    );
    this.secret = meta.get("secret") ?? "";
    this.epoch = meta.get("epoch") ?? "";
    this.devices = machines(meta.get("devices"));
    this.tokens = strings(meta.get("tokens"));
    this.read = cursor(meta.get("read"), 0);
    this.touched = cursor(meta.get("touched"), 0);
    this.flushed =
      sql
        .exec<{ last: number }>(
          "SELECT last FROM out ORDER BY first DESC LIMIT 1"
        )
        .toArray()[0]?.last ?? 0;
    const recent = sql
      .exec<{ id: string; seq: number }>(
        "SELECT seq, id FROM inbox ORDER BY seq DESC LIMIT ?",
        SEEN
      )
      .toArray();
    this.received = (recent[0]?.seq ?? -1) + 1;
    this.seen = recent.map((row) => row.id).filter(Boolean);
  }

  async init(secret: string) {
    this.tables();
    this.secret = secret;
    this.epoch = hex(6);
    this.touched = Date.now();
    this.put("secret", secret);
    this.put("epoch", this.epoch);
    this.put("touched", String(this.touched));
    await this.ctx.storage.setAlarm(this.touched + IDLE);
  }

  async fetch(request: Request) {
    const url = new URL(request.url);
    const action = url.pathname.split("/")[3] ?? "";
    const query = url.searchParams;
    const upgrade =
      request.headers.get("Upgrade")?.toLowerCase() === "websocket";
    if (!this.secret) {
      return upgrade ? refused(4404, GONE) : text(GONE, 404);
    }
    const owner = same(query.get("secret") ?? "", this.secret);
    const device = this.devices.find(
      (known) => known.id === query.get("device")
    );
    if (upgrade) {
      if (action === "in" && owner) {
        return this.mac(query);
      }
      if (action === "events" && device) {
        return this.phone(device, query);
      }
      return refused(4403, UNPAIRED);
    }
    const raw = request.method === "POST" ? await request.text() : "";
    const body = raw.trim();
    if (body.length > BODY) {
      return text("that's too big to send", 413);
    }
    const route = `${request.method} ${action}`;
    if (route === "POST pair") {
      return this.pair(body, request.headers.get("User-Agent") ?? "");
    }
    if (route === "POST send" && device) {
      return this.send(body, query.get("id"));
    }
    return owner ? this.owned(route, body) : text(UNPAIRED, 403);
  }

  private owned(route: string, body: string) {
    switch (route) {
      case "POST token": {
        this.tokens = [...this.tokens, body].slice(-TOKENS);
        this.put("tokens", JSON.stringify(this.tokens));
        return text("ok");
      }
      case "GET devices": {
        return Response.json(this.devices);
      }
      case "POST forget": {
        return this.forget(body);
      }
      case "POST close": {
        return this.close();
      }
      default: {
        return text(UNPAIRED, 403);
      }
    }
  }

  async webSocketMessage(socket: WebSocket, message: string | ArrayBuffer) {
    if (
      typeof message !== "string" ||
      !this.secret ||
      !this.ctx.getTags(socket).includes("mac")
    ) {
      return;
    }
    const { fresh, out, read } = frame(message);
    if (read !== null && read > this.read) {
      this.read = Math.min(read, this.received);
      this.put("read", String(this.read));
    }
    if (fresh) {
      this.restart();
    }
    if (out.length === 0 && !fresh) {
      return;
    }
    this.pending.push(...out);
    tell(
      this.ctx.getWebSockets("phone"),
      JSON.stringify({
        epoch: this.epoch,
        items: out,
        next: this.flushed + this.pending.length,
      })
    );
    if (fresh) {
      this.flush();
    } else if (!this.due) {
      this.due = true;
      await this.ctx.storage.setAlarm(Date.now() + FLUSH);
    }
  }

  webSocketClose(socket: WebSocket) {
    this.left(socket);
  }

  webSocketError(socket: WebSocket) {
    this.left(socket);
  }

  async alarm() {
    this.flush();
    if (!this.secret) {
      return;
    }
    if (this.ctx.getWebSockets().length > 0) {
      this.touch();
    }
    if (Date.now() - this.touched > IDLE) {
      await this.wipe();
      return;
    }
    await this.ctx.storage.setAlarm(this.touched + IDLE);
  }

  private mac(query: URLSearchParams) {
    const pair = new WebSocketPair();
    this.ctx.acceptWebSocket(pair[1], ["mac"]);
    pair[1].serializeAttachment(Date.now());
    pair[1].send(
      JSON.stringify(this.inbox(cursor(query.get("after"), this.read)))
    );
    pair[1].send(JSON.stringify({ devices: this.devices }));
    this.presence(true);
    this.touch();
    return new Response(null, { status: 101, webSocket: pair[0] });
  }

  private phone(device: Device, query: URLSearchParams) {
    const pair = new WebSocketPair();
    this.ctx.acceptWebSocket(pair[1], ["phone", device.id]);
    pair[1].send(
      JSON.stringify({
        epoch: this.epoch,
        mac: this.present(),
        ...this.window(cursor(query.get("after"), 0)),
      })
    );
    return new Response(null, { status: 101, webSocket: pair[0] });
  }

  private pair(token: string, agent: string) {
    const spot = token ? this.tokens.indexOf(token) : -1;
    if (spot === -1) {
      return text(USED, 403);
    }
    this.tokens = this.tokens.filter((_, index) => index !== spot);
    const id = hex(16);
    this.devices = [
      ...this.devices,
      { id, label: label(agent), since: Math.floor(Date.now() / 1000) },
    ];
    this.put("tokens", JSON.stringify(this.tokens));
    this.changed();
    return text(id);
  }

  private send(item: string, id: string | null) {
    if (id && this.seen.includes(id)) {
      return text("ok");
    }
    const seq = this.received;
    this.ctx.storage.sql.exec(
      "INSERT INTO inbox (seq, id, item) VALUES (?, ?, ?)",
      seq,
      id ?? "",
      item
    );
    this.ctx.storage.sql.exec("DELETE FROM inbox WHERE seq < ?", seq - KEPT);
    this.trim("inbox", "seq", "item");
    this.received = seq + 1;
    if (id) {
      this.seen = [id, ...this.seen].slice(0, SEEN);
    }
    tell(
      this.ctx.getWebSockets("mac"),
      JSON.stringify({ items: [item], next: this.received })
    );
    this.touch();
    return text("ok");
  }

  private forget(id: string) {
    this.devices = this.devices.filter((device) => device.id !== id);
    for (const socket of this.ctx.getWebSockets(id)) {
      socket.close(4403, UNPAIRED);
    }
    this.changed();
    return text("ok");
  }

  private async close() {
    for (const socket of this.ctx.getWebSockets()) {
      socket.close(4404, GONE);
    }
    await this.wipe();
    return text("ok");
  }

  private left(socket: WebSocket) {
    if (
      this.secret &&
      this.ctx.getTags(socket).includes("mac") &&
      !this.present()
    ) {
      this.presence(false);
    }
  }

  private present() {
    const now = Date.now();
    return this.ctx.getWebSockets("mac").some((socket) => {
      const pinged =
        this.ctx.getWebSocketAutoResponseTimestamp(socket)?.getTime() ?? 0;
      const joined = cursor(socket.deserializeAttachment(), 0);
      return (
        socket.readyState === WebSocket.OPEN &&
        now - Math.max(pinged, joined) < ALIVE
      );
    });
  }

  private presence(mac: boolean) {
    tell(
      this.ctx.getWebSockets("phone"),
      JSON.stringify({
        epoch: this.epoch,
        items: [],
        mac,
        next: this.flushed + this.pending.length,
      })
    );
  }

  private changed() {
    this.put("devices", JSON.stringify(this.devices));
    tell(
      this.ctx.getWebSockets("mac"),
      JSON.stringify({ devices: this.devices })
    );
  }

  private window(after: number) {
    const { sql } = this.ctx.storage;
    const start =
      sql
        .exec<{ first: number }>(
          "SELECT first FROM out WHERE first <= ? ORDER BY first DESC LIMIT 1",
          after
        )
        .toArray()[0]?.first ?? 0;
    const rows = sql
      .exec<{ first: number; items: string }>(
        "SELECT first, items FROM out WHERE first >= ? ORDER BY first",
        start
      )
      .toArray()
      .map((row) => ({ first: row.first, items: strings(row.items) }));
    return since(append(rebuild(rows, this.flushed), this.pending), after);
  }

  private inbox(after: number) {
    const items = this.ctx.storage.sql
      .exec<{ item: string }>(
        "SELECT item FROM inbox WHERE seq >= ? ORDER BY seq",
        after
      )
      .toArray()
      .map((row) => row.item);
    return { items, next: this.received };
  }

  private restart() {
    this.ctx.storage.sql.exec("DELETE FROM out");
    this.pending = [];
    this.flushed = 0;
    this.epoch = hex(6);
    this.put("epoch", this.epoch);
  }

  private flush() {
    this.due = false;
    if (this.pending.length === 0) {
      return;
    }
    const { sql } = this.ctx.storage;
    let first = this.flushed;
    for (const row of pack(this.pending)) {
      sql.exec(
        "INSERT INTO out (first, last, items) VALUES (?, ?, ?)",
        first,
        first + row.length,
        JSON.stringify(row)
      );
      first += row.length;
    }
    this.flushed = first;
    this.pending = [];
    const oldest = sql
      .exec<{ first: number }>(
        "SELECT first FROM out WHERE first <= ? ORDER BY first DESC LIMIT 1",
        first - KEPT
      )
      .toArray()[0]?.first;
    if (oldest !== undefined) {
      sql.exec("DELETE FROM out WHERE first < ?", oldest);
    }
    this.trim("out", "first", "items");
    this.touch();
  }

  private trim(
    table: "inbox" | "out",
    key: "first" | "seq",
    column: "item" | "items"
  ) {
    const { sql } = this.ctx.storage;
    const rows = sql
      .exec<{ at: number; size: number }>(
        `SELECT ${key} AS at, length(${column}) AS size FROM ${table} ORDER BY ${key}`
      )
      .toArray();
    const drop = over(rows.map((row) => row.size));
    const cut = rows[drop]?.at;
    if (drop > 0 && cut !== undefined) {
      sql.exec(`DELETE FROM ${table} WHERE ${key} < ?`, cut);
    }
  }

  private touch() {
    const now = Date.now();
    if (now - this.touched > HOUR) {
      this.touched = now;
      this.put("touched", String(now));
    }
  }

  private async wipe() {
    this.secret = "";
    this.devices = [];
    this.tokens = [];
    this.pending = [];
    await this.ctx.storage.deleteAlarm();
    await this.ctx.storage.deleteAll();
  }

  private put(key: string, value: string) {
    this.ctx.storage.sql.exec(
      "INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
      key,
      value
    );
  }
}
