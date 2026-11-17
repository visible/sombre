export const ARM = 500;

interface Focused {
  tagName: string;
  isContentEditable?: boolean;
}

const fields = new Set(["INPUT", "SELECT", "TEXTAREA"]);

export const typing = (element: Focused | null | undefined) =>
  element !== null &&
  element !== undefined &&
  (element.isContentEditable === true || fields.has(element.tagName));

export const armed = (since: number, now: number) => now - since >= ARM;
