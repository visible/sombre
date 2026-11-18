const SIGMA = [0x61_70_78_65, 0x33_20_64_6e, 0x79_62_2d_32, 0x6b_20_65_74];
const PRIME = (1n << 130n) - 5n;
const encoder = new TextEncoder();
const decoder = new TextDecoder();
export const COMPUTER = encoder.encode("sombre computer");
export const PHONE = encoder.encode("sombre phone");
const BUCKET = 64;

const rotate = (value, shift) => (value << shift) | (value >>> (32 - shift));

const words = (bytes, count) => {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return Array.from({ length: count }, (_, index) =>
    view.getUint32(index * 4, true)
  );
};

const chacha = new Uint32Array(16);
const mixed = new Uint32Array(16);

const quarter = (a, b, c, d) => {
  mixed[a] += mixed[b];
  mixed[d] = rotate(mixed[d] ^ mixed[a], 16);
  mixed[c] += mixed[d];
  mixed[b] = rotate(mixed[b] ^ mixed[c], 12);
  mixed[a] += mixed[b];
  mixed[d] = rotate(mixed[d] ^ mixed[a], 8);
  mixed[c] += mixed[d];
  mixed[b] = rotate(mixed[b] ^ mixed[c], 7);
};

const prime = (key, counter, nonce) => {
  chacha.set(SIGMA);
  chacha.set(words(key, 8), 4);
  chacha[12] = counter;
  chacha.set(words(nonce, 3), 13);
};

const cycle = () => {
  mixed.set(chacha);
  for (let round = 0; round < 10; round += 1) {
    quarter(0, 4, 8, 12);
    quarter(1, 5, 9, 13);
    quarter(2, 6, 10, 14);
    quarter(3, 7, 11, 15);
    quarter(0, 5, 10, 15);
    quarter(1, 6, 11, 12);
    quarter(2, 7, 8, 13);
    quarter(3, 4, 9, 14);
  }
  for (let index = 0; index < 16; index += 1) {
    mixed[index] += chacha[index];
  }
};

export const block = (key, counter, nonce) => {
  prime(key, counter, nonce);
  cycle();
  const out = new Uint8Array(64);
  const view = new DataView(out.buffer);
  for (let index = 0; index < 16; index += 1) {
    view.setUint32(index * 4, mixed[index], true);
  }
  return out;
};

const stream = (key, nonce, data) => {
  const out = new Uint8Array(data.length);
  prime(key, 1, nonce);
  for (let offset = 0; offset < data.length; offset += 64) {
    cycle();
    const end = Math.min(64, data.length - offset);
    for (let index = 0; index < end; index += 1) {
      const pad = (mixed[index >> 2] >>> ((index & 3) * 8)) & 0xff;
      out[offset + index] = data[offset + index] ^ pad;
    }
    chacha[12] += 1;
  }
  return out;
};

const LIMB = 0x1f_ff;
const SPAN = 8192;

const spread = (bytes, at, into, top) => {
  const pair = (index) =>
    bytes[at + index * 2] | (bytes[at + index * 2 + 1] << 8);
  const t0 = pair(0);
  const t1 = pair(1);
  const t2 = pair(2);
  const t3 = pair(3);
  const t4 = pair(4);
  const t5 = pair(5);
  const t6 = pair(6);
  const t7 = pair(7);
  into[0] += t0 & LIMB;
  into[1] += ((t0 >>> 13) | (t1 << 3)) & LIMB;
  into[2] += ((t1 >>> 10) | (t2 << 6)) & LIMB;
  into[3] += ((t2 >>> 7) | (t3 << 9)) & LIMB;
  into[4] += ((t3 >>> 4) | (t4 << 12)) & LIMB;
  into[5] += (t4 >>> 1) & LIMB;
  into[6] += ((t4 >>> 14) | (t5 << 2)) & LIMB;
  into[7] += ((t5 >>> 11) | (t6 << 5)) & LIMB;
  into[8] += ((t6 >>> 8) | (t7 << 8)) & LIMB;
  into[9] += (t7 >>> 5) | top;
};

const times = (h, r, next) => {
  let carry = 0;
  for (let index = 0; index < 10; index += 1) {
    let sum = carry;
    for (let other = 0; other < 10; other += 1) {
      sum +=
        h[other] *
        (other <= index ? r[index - other] : 5 * r[index + 10 - other]);
    }
    carry = Math.floor(sum / SPAN);
    next[index] = sum & LIMB;
  }
  next[0] += carry * 5;
  carry = Math.floor(next[0] / SPAN);
  next[0] &= LIMB;
  next[1] += carry;
  h.set(next);
};

const little = (bytes) => {
  const view = new DataView(bytes.buffer, bytes.byteOffset, 16);
  return view.getBigUint64(0, true) | (view.getBigUint64(8, true) << 64n);
};

const poly = (key, message) => {
  const clamped = key.slice(0, 16);
  for (const index of [3, 7, 11, 15]) {
    clamped[index] &= 15;
  }
  for (const index of [4, 8, 12]) {
    clamped[index] &= 252;
  }
  const r = new Float64Array(10);
  spread(clamped, 0, r, 0);
  const h = new Float64Array(10);
  const next = new Float64Array(10);
  const last = new Uint8Array(16);
  const whole = message.length - (message.length % 16);
  for (let offset = 0; offset < whole; offset += 16) {
    spread(message, offset, h, 1 << 11);
    times(h, r, next);
  }
  if (whole < message.length) {
    last.set(message.subarray(whole));
    last[message.length - whole] = 1;
    spread(last, 0, h, 0);
    times(h, r, next);
  }
  let total = 0n;
  for (let index = 9; index >= 0; index -= 1) {
    total = total * 8192n + BigInt(h[index]);
  }
  total =
    ((total % PRIME) + little(key.subarray(16, 32))) & ((1n << 128n) - 1n);
  return Uint8Array.from({ length: 16 }, (_, index) =>
    Number((total >> BigInt(8 * index)) & 0xffn)
  );
};

const padded = (length) => (16 - (length % 16)) % 16;

const tag = (key, nonce, aad, sealed) => {
  const head = aad.length + padded(aad.length);
  const body = sealed.length + padded(sealed.length);
  const message = new Uint8Array(head + body + 16);
  message.set(aad);
  message.set(sealed, head);
  const view = new DataView(message.buffer, head + body);
  view.setBigUint64(0, BigInt(aad.length), true);
  view.setBigUint64(8, BigInt(sealed.length), true);
  return poly(block(key, 0, nonce).subarray(0, 32), message);
};

export const encrypt = (key, nonce, aad, plain) => {
  const sealed = stream(key, nonce, plain);
  const framed = new Uint8Array(sealed.length + 16);
  framed.set(sealed);
  framed.set(tag(key, nonce, aad, sealed), sealed.length);
  return framed;
};

export const decrypt = (key, nonce, aad, framed) => {
  const sealed = framed.subarray(0, -16);
  const expected = tag(key, nonce, aad, sealed);
  const given = framed.subarray(-16);
  let difference = 0;
  for (let index = 0; index < 16; index += 1) {
    difference |= expected[index] ^ given[index];
  }
  return difference === 0 ? stream(key, nonce, sealed) : null;
};

export const unbase = (text) => {
  if (Uint8Array.fromBase64) {
    return Uint8Array.fromBase64(text, { alphabet: "base64url" });
  }
  const plain = text.replaceAll("-", "+").replaceAll("_", "/");
  const binary = atob(plain + "=".repeat(padded(plain.length * 4) / 4));
  const out = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    out[index] = binary.codePointAt(index) ?? 0;
  }
  return out;
};

export const encode = (bytes) => {
  if (bytes.toBase64) {
    return bytes.toBase64();
  }
  let text = "";
  for (let start = 0; start < bytes.length; start += 0x80_00) {
    text += String.fromCodePoint(...bytes.subarray(start, start + 0x80_00));
  }
  return btoa(text);
};

export const base = (bytes) =>
  encode(bytes).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");

export const seal = (key, value, side = PHONE) => {
  const nonce = crypto.getRandomValues(new Uint8Array(12));
  const body = encoder.encode(JSON.stringify(value));
  const plain = new Uint8Array(
    Math.max(BUCKET, Math.ceil(body.length / BUCKET) * BUCKET)
  ).fill(32);
  plain.set(body);
  const sealed = encrypt(key, nonce, side, plain);
  const framed = new Uint8Array(nonce.length + sealed.length);
  framed.set(nonce);
  framed.set(sealed, nonce.length);
  return base(framed);
};

export const delay = (tries) =>
  Math.min(5000, 500 * 2 ** Math.max(0, tries - 1));

export const ticket = () => base(crypto.getRandomValues(new Uint8Array(9)));

export const tallied = (steps) => {
  const counts = [];
  for (const step of steps) {
    const found = counts.find((entry) => entry.label === step.label);
    if (found) {
      found.count += 1;
    } else {
      counts.push({ count: 1, label: step.label });
    }
  }
  return counts
    .map(({ count, label }) => (count > 1 ? `${label} ×${count}` : label))
    .join("  ");
};

export const stripped = (text) =>
  text.replaceAll(/\[Image #\d+\]\s*/gu, "").trim();

const LONGEST = 1600;

const shrink = async (file) => {
  try {
    const bitmap = await createImageBitmap(file);
    const scale = Math.min(1, LONGEST / Math.max(bitmap.width, bitmap.height));
    const canvas = new OffscreenCanvas(
      Math.max(1, Math.round(bitmap.width * scale)),
      Math.max(1, Math.round(bitmap.height * scale))
    );
    const context = canvas.getContext("2d");
    context.fillStyle = "#fff";
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    bitmap.close();
    const blob = await canvas.convertToBlob({
      quality: 0.8,
      type: "image/jpeg",
    });
    const data = encode(new Uint8Array(await blob.arrayBuffer()));
    return {
      data,
      name: file.name || "photo.jpg",
      src: `data:image/jpeg;base64,${data}`,
    };
  } catch {
    return null;
  }
};

export const queued = (raw) => {
  try {
    const parsed = JSON.parse(raw ?? "[]");
    return Array.isArray(parsed)
      ? parsed.filter(
          (entry) =>
            typeof entry?.id === "string" &&
            typeof entry?.value?.kind === "string"
        )
      : [];
  } catch {
    return [];
  }
};

export const unsealed = (key, text, side = COMPUTER) => {
  const framed = unbase(text);
  if (framed.length < 28) {
    return null;
  }
  const nonce = framed.slice(0, 12);
  const body = framed.slice(12);
  const directed = decrypt(key, nonce, side, body);
  const plain = directed ?? decrypt(key, nonce, new Uint8Array(), body);
  return plain
    ? { directed: directed !== null, value: JSON.parse(decoder.decode(plain)) }
    : null;
};

export const open = (key, text, side = COMPUTER) =>
  unsealed(key, text, side)?.value ?? null;

const PLACEHOLDER = /(?<placeholder>\[Image #\d+\])/u;
const INLINE =
  /`(?<code>[^`]+)`|\*\*(?<strong>[^*]+)\*\*|\*(?<em>[^*\s][^*]*)\*|\[(?<label>[^\]]+)\]\((?<href>[^\s)]+)\)/gu;
const ITEM = /^\s*(?:[-*+]|\d+[.)])\s+/u;
const HEADING = /^(?<marks>#{1,6})\s+(?<title>.*)$/u;
const RULE = /^(?:-{3,}|\*{3,}|_{3,})\s*$/u;
const BOX = /[─━═]{3,}/gu;
const SHOWN = 5;
const PATIENCE = 35_000;
const GATHER = 100;
const STREAM = 80;
const PHOTOS = 4;
const SENDING = 12_000;
const LOGOS = {
  claude:
    "m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z",
  openai:
    "M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z",
  vercel: "m12 1.608 12 20.784H0Z",
};

const element = (kind, name, text) => {
  const node = document.createElement(kind);
  if (name) {
    node.className = name;
  }
  if (text !== undefined) {
    node.textContent = text;
  }
  return node;
};

const inline = (parent, text) => {
  let last = 0;
  for (const match of text.matchAll(INLINE)) {
    parent.append(text.slice(last, match.index));
    const { code, strong, em, label, href } = match.groups;
    const plain = code?.replaceAll(BOX, "").trim();
    if (plain === "") {
      parent.append(element("hr"));
    } else if (plain !== undefined) {
      parent.append(element("code", "", plain));
    } else if (strong !== undefined) {
      parent.append(element("strong", "", strong));
    } else if (em === undefined && /^https?:\/\//u.test(href)) {
      const link = element("a", "", label);
      link.href = href;
      link.rel = "noreferrer noopener";
      link.target = "_blank";
      parent.append(link);
    } else if (em === undefined) {
      const named = element("span", "", label);
      named.title = href;
      parent.append(named);
    } else {
      parent.append(element("em", "", em));
    }
    last = match.index + match[0].length;
  }
  parent.append(text.slice(last));
  return parent;
};

const fence = (language, body) => {
  const figure = element("figure", "code");
  if (language) {
    figure.append(element("figcaption", "", language));
  }
  const pre = element("pre");
  pre.append(element("code", "", body));
  figure.append(pre);
  return figure;
};

const cells = (row) =>
  row
    .trim()
    .replace(/^\|/u, "")
    .replace(/\|$/u, "")
    .split("|")
    .map((cell) => cell.trim());

const table = (rows) => {
  const wrap = element("div", "grid");
  const grid = element("table");
  const kept = rows.filter((row) => !/^[\s|:-]+$/u.test(row));
  for (const [index, row] of kept.entries()) {
    const line = element("tr");
    for (const cell of cells(row)) {
      line.append(inline(element(index === 0 ? "th" : "td"), cell));
    }
    grid.append(line);
  }
  wrap.append(grid);
  return wrap;
};

const list = (rows) => {
  const node = element(/^\s*\d/u.test(rows[0]) ? "ol" : "ul");
  let current = null;
  for (const row of rows) {
    if (ITEM.test(row) || !current) {
      current = inline(element("li"), row.replace(ITEM, ""));
      node.append(current);
    } else {
      current.append(" ");
      inline(current, row.trim());
    }
  }
  return node;
};

const lines = (parent, rows, strip) => {
  for (const [index, row] of rows.entries()) {
    if (index > 0) {
      parent.append(element("br"));
    }
    inline(parent, strip ? row.replace(strip, "") : row);
  }
  return parent;
};

const opens = (row) =>
  row.startsWith("```") ||
  HEADING.test(row) ||
  RULE.test(row) ||
  row.startsWith(">") ||
  row.trimStart().startsWith("|") ||
  ITEM.test(row);

const markdown = (source) => {
  const out = document.createDocumentFragment();
  const rows = source.replaceAll("\r\n", "\n").split("\n");
  let index = 0;
  const take = (keep) => {
    const kept = [];
    while (index < rows.length && keep(rows[index])) {
      kept.push(rows[index]);
      index += 1;
    }
    return kept;
  };
  while (index < rows.length) {
    const row = rows[index];
    const heading = HEADING.exec(row);
    if (!row.trim()) {
      index += 1;
    } else if (row.startsWith("```")) {
      index += 1;
      const body = take((next) => !next.startsWith("```"));
      index += 1;
      out.append(fence(row.slice(3).trim(), body.join("\n")));
    } else if (heading) {
      const level = heading.groups.marks.length <= 2 ? "h3" : "h4";
      out.append(inline(element(level), heading.groups.title));
      index += 1;
    } else if (RULE.test(row)) {
      out.append(element("hr"));
      index += 1;
    } else if (row.startsWith(">")) {
      const quote = take((next) => next.startsWith(">"));
      out.append(lines(element("blockquote"), quote, /^>\s?/u));
    } else if (row.trimStart().startsWith("|")) {
      out.append(table(take((next) => next.trimStart().startsWith("|"))));
    } else if (ITEM.test(row)) {
      out.append(
        list(
          take(
            (next) =>
              ITEM.test(next) || (next.startsWith("  ") && next.trim() !== "")
          )
        )
      );
    } else {
      index += 1;
      const rest = take((next) => next.trim() !== "" && !opens(next));
      out.append(lines(element("p"), [row, ...rest]));
    }
  }
  return out;
};

const piece = (step) => {
  const row = element("div", "act");
  row.append(element("span", "verb", step.label));
  if (step.detail) {
    const more = element("button", "detail", step.detail);
    more.type = "button";
    more.setAttribute("aria-expanded", "false");
    more.addEventListener("click", () => {
      const shown = row.classList.toggle("open");
      more.setAttribute("aria-expanded", String(shown));
    });
    row.append(more);
  }
  if (step.output) {
    const rows = step.output.split("\n");
    const box = element("pre", "", rows.slice(0, SHOWN).join("\n"));
    row.append(box);
    if (rows.length > SHOWN) {
      const hidden = rows.length - SHOWN;
      const more = element(
        "button",
        "more",
        `… +${hidden} ${hidden === 1 ? "line" : "lines"}`
      );
      more.type = "button";
      more.addEventListener("click", () => {
        box.textContent = step.output;
        more.remove();
      });
      row.append(more);
    }
  }
  return row;
};

const SVG = "http://www.w3.org/2000/svg";

const logo = (agent, model) => {
  if (agent === "claude code") {
    return "claude";
  }
  if (agent === "codex") {
    return "openai";
  }
  if ((model ?? "").startsWith("gateway")) {
    return "vercel";
  }
  return "";
};

const tone = (row) => {
  if (row.startsWith("+")) {
    return "add";
  }
  if (row.startsWith("-")) {
    return "gone";
  }
  return "";
};

export const summary = (message) => {
  if (!(message.cost > 0)) {
    return "";
  }
  return message.cost < 0.01 ? "<$0.01" : `$${message.cost.toFixed(2)}`;
};

const ring = (left) => {
  const used = Math.min(100, Math.max(0, 100 - left));
  const around = 2 * Math.PI * 5;
  const icon = document.createElementNS(SVG, "svg");
  icon.setAttribute("viewBox", "0 0 12 12");
  icon.setAttribute("class", left < 20 ? "ring low" : "ring");
  icon.setAttribute("role", "img");
  icon.setAttribute("aria-label", `${left}% context left`);
  const title = document.createElementNS(SVG, "title");
  title.textContent = `${left}% context left`;
  const track = document.createElementNS(SVG, "circle");
  const fill = document.createElementNS(SVG, "circle");
  for (const circle of [track, fill]) {
    circle.setAttribute("cx", "6");
    circle.setAttribute("cy", "6");
    circle.setAttribute("r", "5");
    circle.setAttribute("fill", "none");
    circle.setAttribute("stroke-width", "2");
  }
  track.setAttribute("class", "track");
  fill.setAttribute("class", "used");
  fill.setAttribute("stroke-dasharray", `${(used / 100) * around} ${around}`);
  fill.setAttribute("transform", "rotate(-90 6 6)");
  icon.append(title, track, fill);
  return icon;
};

const gap = () =>
  document.body.scrollHeight - window.innerHeight - window.scrollY;
const slot = (name) => `${location.pathname.replace(/\/$/u, "")} ${name}`;

const remember = (name, value) => localStorage.setItem(slot(name), value);

const discard = (name) => {
  localStorage.removeItem(slot(name));
  localStorage.removeItem(name);
  sessionStorage.removeItem(name);
};

const recall = (name) => {
  const kept =
    localStorage.getItem(slot(name)) ??
    localStorage.getItem(name) ??
    sessionStorage.getItem(name);
  if (kept !== null) {
    localStorage.setItem(slot(name), kept);
    localStorage.removeItem(name);
  }
  return kept;
};

const field = (asked) => {
  const set = element("fieldset", "question");
  const legend = element("legend");
  if (asked.header) {
    legend.append(element("span", "topic", asked.header));
  }
  legend.append(element("span", "", asked.text));
  set.append(legend);
  const group = ticket();
  const typed = element("input", "other");
  typed.type = "text";
  typed.placeholder = "type your answer";
  typed.maxLength = 500;
  typed.hidden = true;
  typed.setAttribute("aria-label", "your answer");
  const choices = [
    ...asked.choices,
    { about: "", label: "something else", other: true },
  ];
  for (const choice of choices) {
    const label = element("label", "pick");
    const box = element("input");
    box.type = asked.multiple ? "checkbox" : "radio";
    box.name = group;
    box.value = choice.label;
    if (choice.other) {
      box.dataset.other = "";
    }
    const caption = element("span", "words");
    caption.append(element("span", "name", choice.label));
    if (choice.about) {
      caption.append(element("span", "about", choice.about));
    }
    label.append(box, caption);
    set.append(label);
  }
  set.append(typed);
  const other = set.querySelector("[data-other]");
  set.addEventListener("change", () => {
    const closed = typed.hidden;
    typed.hidden = !other.checked;
    if (other.checked && closed) {
      typed.focus();
    }
  });
  const value = () => {
    const picked = [...set.querySelectorAll("input:checked")];
    const text = typed.value.trim();
    if (picked.length === 0 || (other.checked && !text)) {
      return null;
    }
    return picked.map((box) => (box === other ? text : box.value)).join(", ");
  };
  return { set, value };
};

const matches = (card, message) =>
  !message.id || card.dataset.ask === message.id;

const outcome = (yes) => {
  if (yes === null) {
    return element("p", "said", "answered");
  }
  return element("p", yes ? "said" : "said no", yes ? "allowed" : "denied");
};

const near = () => gap() < 160;
const bottom = () => window.scrollTo({ top: document.body.scrollHeight });

const start = () => {
  const feed = document.querySelector("#feed");
  const status = document.querySelector("#status");
  const name = document.querySelector("#name");
  const meta = document.querySelector("#meta");
  const footer = document.querySelector("#footer");
  const rule = document.querySelector("#rule");
  const form = document.querySelector("#composer");
  const input = document.querySelector("#message");
  const queue = document.querySelector("#queue");
  const send = document.querySelector("#send");
  const banner = document.querySelector("#banner");
  const viewer = document.querySelector("#viewer");
  const mark = document.querySelector(".mark");
  const arrow = send.firstElementChild;
  const root = location.pathname.replace(/\/$/u, "");
  const waiting = new Set();
  const outbox = queued(recall("outbox"));
  const pending = [];
  const photos = [];
  const tray = document.querySelector("#tray");
  const picker = document.querySelector("#picker");
  const attach = document.querySelector("#attach");
  const state = {
    after: 0,
    agent: "",
    away: false,
    began: 0,
    busy: false,
    clock: 0,
    device: recall("device"),
    directed: false,
    epoch: "",
    joined: false,
    live: null,
    model: "",
    plan: null,
    stick: true,
    tries: 0,
    work: null,
  };
  const latest = document.querySelector("#latest");
  const release = () => {
    state.stick = false;
  };
  window.addEventListener("scroll", () => {
    if (gap() < 24) {
      state.stick = true;
      latest.hidden = true;
    } else if (!near()) {
      state.stick = false;
    }
  });
  window.addEventListener(
    "wheel",
    (event) => {
      if (event.deltaY < 0) {
        release();
      }
    },
    { passive: true }
  );
  let touched = 0;
  window.addEventListener(
    "touchstart",
    (event) => {
      touched = event.touches[0]?.clientY ?? 0;
    },
    { passive: true }
  );
  window.addEventListener(
    "touchmove",
    (event) => {
      if ((event.touches[0]?.clientY ?? 0) > touched + 4) {
        release();
      }
    },
    { passive: true }
  );
  for (const button of [latest, send, queue]) {
    button.addEventListener("pointerdown", (event) => event.preventDefault());
  }
  latest.addEventListener("click", () => {
    state.stick = true;
    latest.hidden = true;
    bottom();
  });
  const mood = (kind, label) => {
    document.body.dataset.state = kind;
    status.textContent = label;
    if (kind === "connecting" || kind === "off") {
      rule.textContent = label;
    } else if (!state.busy) {
      rule.textContent = "";
    }
  };
  let scrolling = 0;
  const follow = () => {
    if (state.stick) {
      scrolling ||= requestAnimationFrame(() => {
        scrolling = 0;
        bottom();
      });
    } else if (state.joined) {
      latest.hidden = false;
    }
  };
  const draw = (group) => {
    const toggle = (steps) => {
      const button = element("button", "tally", tallied(steps));
      button.type = "button";
      button.setAttribute("aria-expanded", String(group.open));
      button.addEventListener("click", () => {
        group.open = !group.open;
        draw(group);
      });
      return button;
    };
    const fold = (steps) => {
      const calm = steps.filter((step) => !step.output);
      const loud = steps.filter((step) => step.output);
      if (calm.length > 1) {
        return [toggle(calm), ...loud.map(piece)];
      }
      return steps.map(piece);
    };
    if (group.open) {
      group.node.replaceChildren(
        toggle(group.steps),
        ...group.steps.map(piece)
      );
    } else if (group.done) {
      group.node.replaceChildren(...fold(group.steps));
    } else {
      group.node.replaceChildren(
        ...fold(group.steps.slice(0, -1)),
        piece(group.steps.at(-1))
      );
    }
    follow();
  };
  const shut = () => {
    if (state.work) {
      state.work.done = true;
      draw(state.work);
      state.work = null;
    }
  };
  const add = (node) => {
    feed.querySelector(".empty")?.remove();
    if (state.work && node !== state.work.node) {
      shut();
    }
    feed.append(node);
    follow();
    return node;
  };
  const empty = (title, detail) => {
    const box = element("div", "empty");
    box.append(mark.cloneNode(true), element("strong", "", title));
    box.append(element("span", "", detail));
    feed.replaceChildren(box);
    document.body.dataset.ready = "";
  };
  const enlarge = (src) => {
    viewer.firstElementChild.src = src;
    viewer.hidden = false;
  };
  viewer.addEventListener("click", () => {
    viewer.hidden = true;
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      viewer.hidden = true;
    }
  });
  const tile = (image, index) => {
    const numbered = typeof index === "number";
    const called = numbered ? `image ${index + 1}` : image.name || "image";
    if (!image.src) {
      return element("span", "chip", `${called} is too large to show`);
    }
    const button = element("button", "tile");
    button.type = "button";
    button.setAttribute("aria-label", `open ${called}`);
    const picture = element("img");
    picture.src = image.src;
    picture.alt = called;
    picture.addEventListener("load", follow);
    button.append(picture);
    if (numbered) {
      button.append(element("span", "number", String(index + 1)));
    }
    button.addEventListener("click", () => enlarge(image.src));
    return button;
  };
  const attention = () => {
    const card = [...waiting].at(-1);
    banner.hidden = !card;
    if (card) {
      const kind = card.dataset.kind === "questions" ? "question" : "approval";
      banner.textContent = `${kind} waiting · tap to see it`;
      document.title = `${kind} needed · sombre`;
      if (state.joined) {
        navigator.vibrate?.(200);
      }
    } else if (document.title.endsWith("needed · sombre")) {
      document.title = "sombre";
    }
  };
  banner.addEventListener("click", () =>
    [...waiting].at(-1)?.scrollIntoView({ behavior: "smooth", block: "center" })
  );
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden && document.title.startsWith("done")) {
      document.title = "sombre";
    }
  });
  const fragment = new URLSearchParams(location.hash.slice(1));
  if (fragment.get("k")) {
    remember("key", fragment.get("k"));
    remember("token", fragment.get("t") ?? "");
    discard("device");
    state.device = null;
    history.replaceState(null, "", location.pathname);
  }
  const saved = recall("key");
  if (!saved) {
    mood("off", "not paired");
    empty(
      "not paired",
      "pair a phone from sombre on your computer, then scan the code"
    );
    return;
  }
  let key = unbase(saved);
  const known = new Set([...JSON.parse(recall("keys") ?? "[]"), saved]);
  const tickle = () => {
    if (document.body.dataset.state !== "working") {
      return;
    }
    const seconds = Math.round((Date.now() - state.began) / 1000);
    rule.textContent = `working ${seconds}s`;
  };
  const busy = (on) => {
    if (on && !state.busy) {
      state.began = Date.now();
      state.clock = setInterval(tickle, 1000);
      tickle();
    }
    if (!on) {
      clearInterval(state.clock);
      rule.textContent = "";
    }
    state.busy = on;
    mood(on ? "working" : "live", on ? "working" : "live on your computer");
    queue.hidden = !on;
    send.setAttribute("aria-label", on ? "send now" : "send");
    send.replaceChildren(on ? "now" : arrow);
  };
  const away = () =>
    outbox.length > 0
      ? `reconnecting. ${outbox.length} ${outbox.length === 1 ? "message" : "messages"} waiting`
      : "reconnecting";
  const store = () => {
    try {
      remember("outbox", JSON.stringify(outbox));
    } catch {
      remember(
        "outbox",
        JSON.stringify(
          outbox.map((entry) => ({
            ...entry,
            value: { ...entry.value, images: [] },
          }))
        )
      );
    }
  };
  const receipt = (entry) => {
    if (state.away) {
      return "waiting for your computer";
    }
    return entry.queued ? "queued on your computer" : "sent";
  };
  const stamp = (id, text) => {
    const held = pending.find((entry) => entry.id === id);
    if (held) {
      held.state.textContent = text;
      held.node.classList.toggle("failed", text === "not sent");
    }
  };
  let sending = false;
  let retry = 0;
  const deliver = async () => {
    clearTimeout(retry);
    const [next] = outbox;
    if (sending || !next || !state.device) {
      return;
    }
    sending = true;
    const body = seal(
      key,
      next.value,
      state.directed ? PHONE : new Uint8Array()
    );
    try {
      const response = await fetch(
        `${root}/send?device=${state.device}&id=${next.id}`,
        {
          body,
          method: "POST",
          signal: AbortSignal.timeout(SENDING + body.length / 40),
        }
      );
      if (response.status >= 500) {
        throw new Error(response.statusText);
      }
      outbox.shift();
      store();
      if (response.ok) {
        stamp(next.id, receipt(next));
      } else {
        stamp(next.id, "not sent");
        add(element("p", "oops", await response.text()));
      }
      sending = false;
      deliver();
    } catch {
      sending = false;
      state.tries += 1;
      mood("connecting", away());
      retry = setTimeout(deliver, delay(state.tries));
    }
  };
  const settle = () => {
    shut();
    state.live = null;
  };
  const you = (message) => {
    settle();
    const box = element("div", "you");
    const images = message.images ?? [];
    const text =
      images.length > 0
        ? message.text.replaceAll(/\[Image #\d+\]\s*/gu, "").trim()
        : message.text;
    const bubble = element("p");
    for (const part of text.split(PLACEHOLDER)) {
      if (PLACEHOLDER.test(part)) {
        bubble.append(element("span", "chip", "image"));
      } else {
        bubble.append(part);
      }
    }
    if (bubble.textContent.trim() || bubble.childElementCount > 0) {
      box.append(bubble);
    }
    if (images.length > 0) {
      const row = element("div", "pics");
      row.append(...images.map(tile));
      box.append(row);
    }
    return add(box);
  };
  const hold = (id, text, images = []) => {
    const node = you({ images, text });
    node.classList.add("pending");
    const shown = element("span", "state", "sending");
    node.append(shown);
    pending.push({ id, images, node, state: shown, text });
  };
  const echo = (message) => {
    if (!(message.again || message.command)) {
      busy(true);
    }
    const spot = message.again
      ? -1
      : pending.findIndex(
          (entry) => stripped(entry.text) === stripped(message.text)
        );
    const [held] = spot >= 0 ? pending.splice(spot, 1) : [];
    held?.node.remove();
    const lost = held?.images.length > 0 && (message.images ?? []).length === 0;
    you(lost ? { ...message, images: held.images } : message);
    if (lost) {
      add(
        element(
          "p",
          "note",
          "these photos didn't reach your computer. update sombre there and restart it"
        )
      );
    }
  };
  const say = (text) => {
    if (!state.live) {
      shut();
      state.live = { drawn: false, node: add(element("div", "say")), text: "" };
    }
    state.live.text = text;
    feed.setAttribute("aria-busy", "true");
    if (!state.live.drawn) {
      state.live.drawn = true;
      const { live } = state;
      setTimeout(() => {
        live.drawn = false;
        live.node.replaceChildren(markdown(live.text));
        follow();
      }, STREAM);
    }
  };
  const work = () => {
    if (!state.work || feed.lastElementChild !== state.work.node) {
      state.live = null;
      feed.removeAttribute("aria-busy");
      const group = {
        done: false,
        node: element("div", "work"),
        open: false,
        steps: [],
      };
      add(group.node);
      state.work = group;
    }
    return state.work;
  };
  const redraw = (group) => {
    if (!group.pending) {
      group.pending = true;
      requestAnimationFrame(() => {
        group.pending = false;
        draw(group);
      });
    }
  };
  const act = (label, detail) => {
    const group = work();
    group.steps.push({ detail, label, output: "" });
    redraw(group);
  };
  const output = (text) => {
    const step = state.work?.steps.at(-1);
    if (!step) {
      add(element("pre", "printed", text));
      return;
    }
    step.output = text;
    redraw(state.work);
  };
  const plan = (steps) => {
    settle();
    const card = element("section", "plan");
    const items = element("ol");
    for (const [step, done] of steps) {
      items.append(element("li", done, step));
    }
    card.append(element("p", "", "plan"), items);
    if (state.plan?.isConnected) {
      state.plan.replaceWith(card);
    } else {
      add(card);
    }
    state.plan = card;
  };
  const picture = (image) => {
    settle();
    const figure = element("figure", "pic");
    figure.append(tile(image));
    if (image.name) {
      figure.append(element("figcaption", "", image.name));
    }
    add(figure);
  };
  const close = (card, yes) => {
    for (const button of card.querySelectorAll("button")) {
      button.disabled = true;
    }
    card.querySelector(".choices")?.replaceWith(outcome(yes));
    waiting.delete(card);
  };
  const answered = (card, answers) => {
    for (const control of card.querySelectorAll("input, button")) {
      control.disabled = true;
    }
    const result = element("div", "answers");
    if (answers) {
      for (const answer of answers) {
        result.append(element("p", "said", answer));
      }
    } else {
      result.append(element("p", "said no", "skipped"));
    }
    card.querySelector(".choices")?.replaceWith(result);
    waiting.delete(card);
  };
  const decide = (yes) => {
    for (const card of waiting) {
      if (card.dataset.kind === "questions") {
        answered(card, null);
      } else {
        close(card, yes);
      }
    }
    attention();
  };
  const questions = (message) => {
    settle();
    const card = element("section", "ask");
    card.dataset.kind = "questions";
    card.dataset.ask = message.id ?? "";
    card.setAttribute("aria-live", "assertive");
    card.append(element("h2", "", `${state.agent || "your agent"} asks`));
    const fields = message.questions.map(field);
    card.append(...fields.map((each) => each.set));
    const row = element("div", "choices");
    const skip = element("button", "key", "skip");
    const submit = element("button", "key yes", "send");
    skip.type = "button";
    submit.type = "button";
    const ready = () => {
      submit.disabled = !fields.every((each) => each.value());
    };
    card.addEventListener("input", ready);
    card.addEventListener("change", ready);
    ready();
    waiting.add(card);
    const reply = async (answers) => {
      answered(card, answers);
      attention();
      outbox.push({
        id: ticket(),
        value: { answers, at: Date.now(), id: message.id, kind: "reply" },
      });
      store();
      await deliver();
    };
    skip.addEventListener("click", () => reply(null));
    submit.addEventListener("click", () =>
      reply(fields.map((each) => each.value()))
    );
    row.append(skip, submit);
    card.append(row);
    add(card);
    attention();
  };
  const standby = () => {
    settle();
    const card = element("section", "ask");
    card.setAttribute("aria-live", "assertive");
    card.append(
      element("h2", "", "sombre stopped on your computer"),
      element("p", "note", "the sombre app there can pick this chat up")
    );
    const row = element("div", "choices");
    const go = element("button", "key yes", "continue in the app");
    go.type = "button";
    go.addEventListener("click", async () => {
      go.disabled = true;
      go.textContent = "opening it on your computer";
      outbox.push({
        id: ticket(),
        value: { at: Date.now(), kind: "continue" },
      });
      store();
      await deliver();
    });
    row.append(go);
    card.append(row);
    add(card);
    attention();
  };
  const ask = (message) => {
    settle();
    const command = message.tool === "Bash";
    const card = element("section", "ask");
    card.dataset.ask = message.id ?? "";
    card.setAttribute("aria-live", "assertive");
    card.append(
      element("h2", "", command ? "run this command?" : "make this edit?"),
      element("pre", "", command ? `$ ${message.detail}` : message.detail)
    );
    if (message.change?.length) {
      const diff = element("pre", "diff");
      for (const row of message.change.slice(0, 14)) {
        diff.append(element("span", tone(row), row));
      }
      card.append(diff);
    }
    const row = element("div", "choices");
    const deny = element("button", "key", "no");
    const allow = element("button", "key yes", "yes");
    deny.type = "button";
    allow.type = "button";
    waiting.add(card);
    const choose = (yes) => async () => {
      close(card, yes);
      attention();
      outbox.push({
        id: ticket(),
        value: { at: Date.now(), id: message.id, kind: "answer", yes },
      });
      store();
      await deliver();
    };
    deny.addEventListener("click", choose(false));
    allow.addEventListener("click", choose(true));
    row.append(deny, allow);
    card.append(row);
    add(card);
    attention();
  };
  const hello = (message) => {
    if (message.fresh) {
      feed.replaceChildren();
      waiting.clear();
      state.plan = null;
      state.joined = false;
    }
    state.agent = message.agent ?? "";
    state.model = message.model ?? "";
    name.textContent = message.name || "sombre";
    meta.textContent = [state.agent, state.model].filter(Boolean).join("  ");
    const brand = element("span", "brand");
    const glyph = logo(state.agent, state.model);
    if (glyph) {
      const icon = document.createElementNS(SVG, "svg");
      icon.setAttribute("viewBox", "0 0 24 24");
      icon.setAttribute("aria-hidden", "true");
      const path = document.createElementNS(SVG, "path");
      path.setAttribute("d", LOGOS[glyph]);
      path.setAttribute("fill", "currentColor");
      icon.append(path);
      brand.append(icon, element("span", "sr", state.agent));
    } else {
      brand.append(state.agent);
    }
    footer.replaceChildren(
      brand,
      element("span", "where", message.folder ?? "")
    );
  };
  const finished = (message) => {
    feed.removeAttribute("aria-busy");
    if (!message.again) {
      busy(false);
      decide(null);
      if (document.hidden) {
        document.title = "done · sombre";
      }
    }
    settle();
    state.plan = null;
    const line = element("p", "done", summary(message));
    if (message.left !== null && message.left !== undefined) {
      line.append(line.textContent ? "  " : "", ring(message.left));
    }
    if (line.textContent || line.childElementCount > 0) {
      add(line);
    }
  };
  const forget = (kind, id) => {
    for (let at = outbox.length - 1; at >= (sending ? 1 : 0); at -= 1) {
      const { value } = outbox[at];
      if (value.kind === kind && value.id === id) {
        outbox.splice(at, 1);
      }
    }
    store();
  };
  const settled = (message) => {
    forget("reply", message.id);
    for (const card of waiting) {
      if (card.dataset.kind === "questions" && matches(card, message)) {
        answered(card, message.answers);
      }
    }
    attention();
  };
  const decided = (message) => {
    forget("answer", message.id);
    for (const card of waiting) {
      if (card.dataset.kind !== "questions" && matches(card, message)) {
        close(card, message.yes);
      }
    }
    attention();
  };
  const handlers = { answered: settled, decided, questions };
  const show = (message) => {
    if (Object.hasOwn(handlers, message.kind)) {
      handlers[message.kind](message);
      return;
    }
    switch (message.kind) {
      case "standby": {
        standby();
        break;
      }
      case "hello": {
        hello(message);
        break;
      }
      case "user": {
        decide(null);
        echo(message);
        break;
      }
      case "delta": {
        say((state.live?.text ?? "") + message.text);
        break;
      }
      case "text": {
        say(message.text);
        state.live = null;
        break;
      }
      case "step": {
        act(message.label, message.detail);
        break;
      }
      case "output": {
        output(message.text);
        break;
      }
      case "plan": {
        plan(message.steps);
        break;
      }
      case "image": {
        picture(message.image);
        break;
      }
      case "approval": {
        ask(message);
        break;
      }
      case "done": {
        finished(message);
        break;
      }
      case "failed": {
        busy(false);
        decide(null);
        settle();
        add(element("p", "oops", message.text));
        break;
      }
      case "printed": {
        settle();
        add(element("pre", "printed", message.text));
        break;
      }
      case "note": {
        settle();
        add(element("p", "note", message.text));
        break;
      }
      default:
    }
  };
  const arrive = () => {
    if (feed.childElementCount === 0 && outbox.length === 0) {
      empty("connected to your computer", "messages in this chat show up here");
    }
    for (const entry of outbox) {
      if (entry.value.kind === "message") {
        hold(
          entry.id,
          entry.value.text,
          (entry.value.images ?? []).map((image) => ({
            name: image.name,
            src: `data:image/jpeg;base64,${image.data}`,
          }))
        );
      }
    }
    bottom();
    document.body.dataset.ready = "";
  };
  let poll = null;
  let timer = 0;
  const unpaired = (title) => {
    for (const stored of ["key", "keys", "device", "token", "outbox"]) {
      discard(stored);
    }
    outbox.length = 0;
    mood("off", "disconnected");
    form.hidden = true;
    empty(title, "pair again on your computer for a new link");
  };
  const received = (body) => {
    state.tries = 0;
    if (state.epoch && body.epoch !== state.epoch) {
      state.epoch = body.epoch;
      state.after = 0;
      state.joined = false;
      state.plan = null;
      waiting.clear();
      attention();
      settle();
      feed.replaceChildren();
      pending.length = 0;
      busy(false);
      return false;
    }
    state.epoch = body.epoch;
    for (const item of body.items) {
      const opened = unsealed(key, item);
      state.directed ||= opened?.directed ?? false;
      const message = opened?.value;
      if (message?.kind === "rekey") {
        if (!known.has(message.key)) {
          known.add(message.key);
          key = unbase(message.key);
          remember("key", message.key);
          remember("keys", JSON.stringify([...known].slice(-8)));
        }
      } else if (message && !(message.again && state.joined)) {
        show(message);
      }
    }
    if (!(body.more || state.joined)) {
      arrive();
    }
    state.joined ||= !body.more;
    state.after = body.next;
    if ("mac" in body) {
      state.away = !body.mac;
    }
    if (state.away) {
      mood("connecting", "waiting for your computer");
    } else if (state.busy) {
      mood("working", "working");
    } else {
      mood("live", "live on your computer");
    }
    deliver();
    return true;
  };
  const listen = async () => {
    clearTimeout(timer);
    poll?.abort();
    const control = new AbortController();
    poll = control;
    const watchdog = setTimeout(() => control.abort(), PATIENCE);
    try {
      const response = await fetch(
        `${root}/events?device=${state.device}&after=${state.after}`,
        { cache: "no-store", signal: control.signal }
      );
      if (response.status === 403) {
        unpaired("this phone isn't paired anymore");
        return;
      }
      if (response.status === 404) {
        unpaired("this chat isn't paired anymore");
        return;
      }
      const body = await response.json();
      if (poll !== control) {
        return;
      }
      received(body);
      timer = setTimeout(listen, body.items.length > 0 ? GATHER : 0);
    } catch {
      if (poll !== control) {
        return;
      }
      state.tries += 1;
      mood("connecting", away());
      timer = setTimeout(listen, delay(state.tries));
    } finally {
      clearTimeout(watchdog);
    }
  };
  let socket = null;
  let heard = 0;
  const subscribe = () => {
    clearTimeout(timer);
    const old = socket;
    socket = null;
    old?.close();
    const scheme = location.protocol === "https:" ? "wss" : "ws";
    const current = new WebSocket(
      `${scheme}://${location.host}${root}/events?device=${state.device}&after=${state.after}`
    );
    socket = current;
    heard = Date.now();
    current.addEventListener("message", (event) => {
      if (socket !== current) {
        return;
      }
      heard = Date.now();
      if (event.data !== "pong" && !received(JSON.parse(event.data))) {
        subscribe();
      }
    });
    current.addEventListener("close", (event) => {
      if (socket !== current) {
        return;
      }
      socket = null;
      if (event.code === 4403) {
        unpaired("this phone isn't paired anymore");
        return;
      }
      if (event.code === 4404) {
        unpaired("this chat isn't paired anymore");
        return;
      }
      state.tries += 1;
      mood("connecting", away());
      timer = setTimeout(subscribe, delay(state.tries));
    });
  };
  const nudge = () => {
    if (
      socket?.readyState === WebSocket.OPEN &&
      Date.now() - heard < PATIENCE / 2
    ) {
      socket.send("ping");
      return;
    }
    if (socket?.readyState !== WebSocket.CONNECTING) {
      subscribe();
    }
  };
  const socketed =
    document.querySelector('meta[name="relay"]')?.content === "socket";
  const hear = socketed ? nudge : listen;
  if (socketed) {
    setInterval(() => {
      if (socket?.readyState !== WebSocket.OPEN || document.hidden) {
        return;
      }
      if (Date.now() - heard > PATIENCE) {
        subscribe();
      } else {
        socket.send("ping");
      }
    }, PATIENCE / 3);
  }
  const kick = () => {
    if (document.hidden || !state.device || form.hidden) {
      return;
    }
    hear();
    deliver();
  };
  window.addEventListener("online", kick);
  window.addEventListener("pageshow", kick);
  document.addEventListener("visibilitychange", kick);
  const grow = () => {
    input.style.height = "auto";
    input.style.height = `${input.scrollHeight}px`;
    document.body.toggleAttribute(
      "data-typed",
      input.value.trim() !== "" || photos.length > 0
    );
  };
  const shelf = () => {
    tray.replaceChildren(
      ...photos.map((photo, index) => {
        const box = element("div", "thumb");
        const preview = element("img");
        preview.src = photo.src;
        preview.alt = `image ${index + 1}`;
        const drop = element("button", "drop", "×");
        drop.type = "button";
        drop.setAttribute("aria-label", `remove image ${index + 1}`);
        drop.addEventListener("click", () => {
          photos.splice(photos.indexOf(photo), 1);
          shelf();
          grow();
        });
        box.append(preview, element("span", "number", String(index + 1)), drop);
        return box;
      })
    );
    tray.hidden = photos.length === 0;
  };
  attach.addEventListener("click", () => picker.click());
  picker.addEventListener("change", async () => {
    const files = [...picker.files].slice(0, PHOTOS - photos.length);
    picker.value = "";
    const made = await Promise.all(files.map(shrink));
    photos.push(...made.filter(Boolean));
    if (made.includes(null)) {
      add(element("p", "note", "couldn't read one of those images"));
    }
    shelf();
    grow();
  });
  const submit = async (now) => {
    const text = input.value.trim();
    if (!text && photos.length === 0) {
      return;
    }
    const images = photos.splice(0);
    input.value = "";
    shelf();
    grow();
    state.stick = true;
    const id = ticket();
    hold(
      id,
      text,
      images.map((image) => ({ name: image.name, src: image.src }))
    );
    outbox.push({
      id,
      queued: state.busy && !now,
      value: {
        at: Date.now(),
        images: images.map((image) => ({ data: image.data, name: image.name })),
        kind: "message",
        now,
        text,
      },
    });
    store();
    bottom();
    await deliver();
  };
  let keyboard = 0;
  const lift = () => {
    const view = window.visualViewport;
    const raw =
      view && document.activeElement === input
        ? Math.round(window.innerHeight - view.height - view.offsetTop)
        : 0;
    const covered = raw > 120 ? raw : 0;
    if (covered === keyboard) {
      return;
    }
    keyboard = covered;
    document.documentElement.style.setProperty("--keyboard", `${covered}px`);
    if (document.activeElement === input && state.stick) {
      bottom();
    }
  };
  new ResizeObserver(() => {
    document.documentElement.style.setProperty(
      "--dock",
      `${form.offsetHeight}px`
    );
    if (state.stick) {
      bottom();
    }
  }).observe(form);
  window.visualViewport?.addEventListener("resize", lift);
  window.visualViewport?.addEventListener("scroll", lift);
  input.addEventListener("focus", () => setTimeout(lift, 300));
  input.addEventListener("blur", lift);
  input.addEventListener("input", grow);
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      submit(state.busy);
    }
  });
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    submit(state.busy);
  });
  queue.addEventListener("click", () => submit(false));
  const connect = async () => {
    if (!state.device) {
      try {
        const response = await fetch(`${root}/pair`, {
          body: recall("token") ?? "",
          method: "POST",
          signal: AbortSignal.timeout(SENDING),
        });
        if (!response.ok) {
          mood("off", "not paired");
          empty("not paired", await response.text());
          return;
        }
        state.device = await response.text();
        remember("device", state.device);
      } catch {
        state.tries += 1;
        mood("connecting", "connecting");
        setTimeout(connect, delay(state.tries));
        return;
      }
    }
    form.hidden = false;
    busy(false);
    hear();
  };
  connect();
};

if (typeof document !== "undefined") {
  start();
}
