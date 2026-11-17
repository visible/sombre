import type { Question } from "./api";

export interface Draft {
  picked: string[];
  other: boolean;
  typed: string;
}

export const blank = (): Draft => ({ other: false, picked: [], typed: "" });

export const toggle = (draft: Draft, label: string, multiple: boolean) => {
  if (!multiple) {
    return { ...draft, other: false, picked: [label] };
  }
  const picked = draft.picked.includes(label)
    ? draft.picked.filter((each) => each !== label)
    : [...draft.picked, label];
  return { ...draft, picked };
};

export const elsewise = (draft: Draft, multiple: boolean) =>
  multiple
    ? { ...draft, other: !draft.other }
    : { ...draft, other: true, picked: [] };

export const value = (question: Question, draft: Draft) => {
  const typed = draft.typed.trim();
  if (draft.other && typed === "") {
    return null;
  }
  const words = [
    ...question.choices
      .map((choice) => choice.label)
      .filter((label) => draft.picked.includes(label)),
    ...(draft.other ? [typed] : []),
  ];
  return words.length > 0 ? words.join(", ") : null;
};
