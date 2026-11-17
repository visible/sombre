import { readdir } from "node:fs/promises";

const source = `${import.meta.dir}/../../crates/cli/src`;
const target = `${import.meta.dir}/public`;
const statics = `${import.meta.dir}/static`;

const names = await readdir(statics);

await Promise.all([
  Bun.write(`${target}/phone.html`, Bun.file(`${source}/phone.html`)),
  Bun.write(`${target}/app.js`, Bun.file(`${source}/phone.js`)),
  ...names.map((name) =>
    Bun.write(`${target}/${name}`, Bun.file(`${statics}/${name}`))
  ),
]);
