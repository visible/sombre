import { describe, expect, test } from "bun:test";

import {
  COMPUTER,
  PHONE,
  decrypt,
  delay,
  encode,
  encrypt,
  open,
  queued,
  seal,
  stripped,
  summary,
  tallied,
  ticket,
  unsealed,
} from "./phone.js";

const bytes = (hex: string) =>
  Uint8Array.from(hex.match(/../gu) ?? [], (pair) => Number.parseInt(pair, 16));

const hex = (data: Uint8Array) =>
  [...data].map((byte) => byte.toString(16).padStart(2, "0")).join("");

describe("chacha20-poly1305 on the phone", () => {
  test("matches the rfc 8439 aead test vector", () => {
    const key = bytes(
      "808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f"
    );
    const nonce = bytes("070000004041424344454647");
    const aad = bytes("50515253c0c1c2c3c4c5c6c7");
    const plain = new TextEncoder().encode(
      "Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it."
    );
    const sealed = encrypt(key, nonce, aad, plain);
    expect(hex(sealed.slice(-16))).toBe("1ae10b594f09e26a7e902ecbd0600691");
    expect(hex(sealed.slice(0, 16))).toBe("d31a8d34648e60db7b86afbc53ef7ec2");
    expect(
      new TextDecoder().decode(
        decrypt(key, nonce, aad, sealed) ?? new Uint8Array()
      )
    ).toBe(new TextDecoder().decode(plain));
  });

  test("matches openssl at every block edge", () => {
    const key = Uint8Array.from({ length: 32 }, (_, index) => index);
    const nonce = Uint8Array.from({ length: 12 }, (_, index) => 0x40 + index);
    const aad = new TextEncoder().encode("sombre computer");
    const vectors: [number, string, string][] = [
      [0, "28c5542d80627fa79e056fb05286ee48", ""],
      [1, "b607b7e2b6351436dcceb619d53f2d9c", "ff"],
      [15, "a90bcbdf177a457e3d7c530993ed8ba3", "ff7239e5f1e82fe1"],
      [16, "f0c34fc87f471b1ab867a229ec1605ec", "ff7239e5f1e82fe1"],
      [17, "f9443faa414cc1f63679232ac5fb0d55", "ff7239e5f1e82fe1"],
      [63, "d38f0544c1550e981ae7feacf35a1c0f", "ff7239e5f1e82fe1"],
      [64, "33fa102c049e95a7d9e1c957bf5ff2af", "ff7239e5f1e82fe1"],
      [65, "135fd927bd237cba6d59bbba8948c03f", "ff7239e5f1e82fe1"],
      [127, "6ce400586c11f85e3c4c57f71e645f1f", "ff7239e5f1e82fe1"],
      [128, "101d62d1dd603471210bed9707649a85", "ff7239e5f1e82fe1"],
      [129, "54a28e01b94b0f782879a3ac070a725a", "ff7239e5f1e82fe1"],
      [1000, "88e5d421ac3bacdbda1a386339bc3950", "ff7239e5f1e82fe1"],
      [4096, "b1507ea4053172fddbb68da7cbaff820", "ff7239e5f1e82fe1"],
    ];
    for (const [length, expected, start] of vectors) {
      const plain = Uint8Array.from(
        { length },
        (_, index) => (index * 31 + 7) % 256
      );
      const sealed = encrypt(key, nonce, aad, plain);
      expect(hex(sealed.subarray(-16))).toBe(expected);
      expect(hex(sealed.subarray(0, Math.min(8, length)))).toBe(start);
      expect(decrypt(key, nonce, aad, sealed)).toEqual(plain);
    }
  });

  test("seals messages only the same key can open", () => {
    const key = crypto.getRandomValues(new Uint8Array(32));
    const sealed = seal(key, { kind: "answer", yes: true });
    expect(sealed).not.toContain("answer");
    expect(open(key, sealed, PHONE)).toEqual({ kind: "answer", yes: true });
    expect(
      open(crypto.getRandomValues(new Uint8Array(32)), sealed, PHONE)
    ).toBeNull();
  });

  test("a message only opens in the direction it was sent", () => {
    const key = crypto.getRandomValues(new Uint8Array(32));
    const fromComputer = seal(key, { kind: "text", text: "hi" }, COMPUTER);
    expect(open(key, fromComputer)).toEqual({ kind: "text", text: "hi" });
    expect(open(key, seal(key, { kind: "answer", yes: true }))).toBeNull();
    const older = seal(key, { kind: "text", text: "old" }, new Uint8Array());
    expect(open(key, older)).toEqual({ kind: "text", text: "old" });
    expect(unsealed(key, fromComputer)?.directed).toBe(true);
    expect(unsealed(key, older)?.directed).toBe(false);
  });

  test("a yes and a no seal to the same length", () => {
    const key = crypto.getRandomValues(new Uint8Array(32));
    const yes = seal(key, { id: "r1", kind: "answer", yes: true });
    const no = seal(key, { id: "r1", kind: "answer", yes: false });
    expect(yes.length).toBe(no.length);
    expect(open(key, no, PHONE)).toEqual({
      id: "r1",
      kind: "answer",
      yes: false,
    });
  });
});

describe("reconnecting on a bad network", () => {
  test("backs off from half a second up to five", () => {
    expect([0, 1, 2, 3, 4, 5, 9].map(delay)).toEqual([
      500, 500, 1000, 2000, 4000, 5000, 5000,
    ]);
  });

  test("gives every message its own id", () => {
    const ids = new Set(Array.from({ length: 50 }, ticket));
    expect(ids.size).toBe(50);
    expect([...ids].every((id) => /^[\w-]{12}$/u.test(id))).toBe(true);
  });

  test("keeps only well formed waiting messages", () => {
    expect(queued(null)).toEqual([]);
    expect(queued("not json")).toEqual([]);
    expect(queued('{"id":"x"}')).toEqual([]);
    expect(
      queued(
        '[{"id":"a","value":{"kind":"message","now":false,"text":"hi"}},{"id":"b","value":{"kind":"answer","yes":true}},{"id":1},{"value":{}}]'
      )
    ).toEqual([
      { id: "a", value: { kind: "message", now: false, text: "hi" } },
      { id: "b", value: { kind: "answer", yes: true } },
    ]);
  });
});

describe("photos from the phone", () => {
  test("encodes large images to base64 in chunks", () => {
    const data = Uint8Array.from(
      { length: 100_000 },
      (_, index) => index % 256
    );
    expect(encode(data)).toBe(Buffer.from(data).toString("base64"));
  });

  test("matches a sent message to its echo without image markers", () => {
    expect(stripped("what is this\n[Image #1] [Image #2]")).toBe(
      "what is this"
    );
    expect(stripped("[Image #1]")).toBe("");
    expect(stripped("  hi  ")).toBe("hi");
  });
});

describe("steps on the phone", () => {
  test("tallies a run of steps the way the cli does", () => {
    expect(
      tallied([
        { label: "Read" },
        { label: "Run" },
        { label: "Read" },
        { label: "Edit" },
        { label: "Read" },
      ])
    ).toBe("Read ×3  Run  Edit");
    expect(tallied([{ label: "Run" }])).toBe("Run");
  });
});

describe("summary", () => {
  test("shows how long the turn took the way the computer does", () => {
    expect(summary({ cost: 1.5, millis: 400 })).toBe("<1s");
    expect(summary({ millis: 6200 })).toBe("6s");
    expect(summary({ millis: 130_000 })).toBe("2m 10s");
    expect(summary({ millis: 3_900_000 })).toBe("1h 05m");
    expect(summary({ cost: 0.5 })).toBe("");
  });
});
