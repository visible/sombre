import { DurableObject } from "cloudflare:workers";

import { DAY, allowed } from "./log";
import type { Env } from "./room";

export class Gate extends DurableObject<Env> {
  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    ctx.storage.sql.exec(
      "CREATE TABLE IF NOT EXISTS stamps (at INTEGER NOT NULL)"
    );
  }

  async admit(now: number) {
    const { sql } = this.ctx.storage;
    sql.exec("DELETE FROM stamps WHERE at <= ?", now - DAY);
    const stamps = sql
      .exec<{ at: number }>("SELECT at FROM stamps")
      .toArray()
      .map((row) => row.at);
    if (!allowed(stamps, now)) {
      return false;
    }
    sql.exec("INSERT INTO stamps (at) VALUES (?)", now);
    await this.ctx.storage.setAlarm(now + DAY);
    return true;
  }

  async alarm() {
    await this.ctx.storage.deleteAll();
  }
}
