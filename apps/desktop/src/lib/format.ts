export const took = (millis: number | null) => {
  if (millis === null) {
    return "";
  }
  const seconds = Math.floor(millis / 1000);
  if (seconds < 1) {
    return "<1s";
  }
  if (seconds < 60) {
    return `${seconds}s`;
  }
  if (seconds < 3600) {
    return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  }
  const minutes = String(Math.floor(seconds / 60) % 60).padStart(2, "0");
  return `${Math.floor(seconds / 3600)}h ${minutes}m`;
};

export const short = (model: string) =>
  model.startsWith("claude-")
    ? model.slice("claude-".length).replace("-", " ")
    : model;

export const clip = (text: string, shown: number) => {
  const lines = text.replace(/\n+$/u, "").split("\n");
  return {
    hidden: Math.max(0, lines.length - shown),
    lines: lines.slice(0, shown),
  };
};

export type Tone = "add" | "gone" | "same";

const tone = (line: string): Tone => {
  if (line.startsWith("+")) {
    return "add";
  }
  if (line.startsWith("-")) {
    return "gone";
  }
  return "same";
};

export const diff = (lines: string[]) =>
  lines.map((text, key) => ({ key, text, tone: tone(text) }));

export const encode = (bytes: Uint8Array) => {
  let text = "";
  for (let start = 0; start < bytes.length; start += 0x80_00) {
    text += String.fromCodePoint(...bytes.subarray(start, start + 0x80_00));
  }
  return btoa(text);
};

export const pictures = (paths: string[]) =>
  paths.filter((path) => /\.(?:png|jpe?g|gif|webp)$/iu.test(path));
