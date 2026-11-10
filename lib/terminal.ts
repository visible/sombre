export type Tone = "command" | "output" | "done" | "ask" | "add" | "remove";

export interface Row {
  tone: Tone;
  text: string;
  aside?: string;
}
