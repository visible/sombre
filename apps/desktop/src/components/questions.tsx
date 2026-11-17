import type { KeyboardEvent } from "react";
import { useEffect, useRef, useState } from "react";

import { armed, typing } from "../lib/arm";
import type { Quiz } from "../lib/live";
import type { Draft } from "../lib/quiz";
import { blank, elsewise, toggle, value } from "../lib/quiz";

interface Props {
  entry: Quiz;
  agent: string;
  onReply: (answers: string[] | null) => void;
}

const OTHER = "something else";

export const Questions = ({ entry, agent, onReply }: Props) => {
  const [drafts, setDrafts] = useState<Draft[]>(() =>
    entry.questions.map(blank)
  );
  const first = useRef<HTMLInputElement>(null);
  const fields = useRef(new Map<number, HTMLInputElement>());
  const opened = useRef<number | null>(null);
  const since = useRef(Number.POSITIVE_INFINITY);

  useEffect(() => {
    if (entry.settled) {
      return;
    }
    since.current = performance.now();
    if (!typing(document.activeElement)) {
      first.current?.focus();
    }
  }, [entry.settled]);

  useEffect(() => {
    if (opened.current !== null) {
      fields.current.get(opened.current)?.focus();
      opened.current = null;
    }
  });

  if (entry.settled) {
    return (
      <div className="flex flex-col pl-[2ch]">
        {entry.answers ? (
          entry.questions.map((question, index) => (
            <p className="text-paper" key={`${entry.ask}-${question.text}`}>
              <span aria-hidden="true" className="text-ember">
                ✔{" "}
              </span>
              {entry.answers?.[index] ?? ""}
              <span className="text-fog ml-[2ch]">{question.text}</span>
            </p>
          ))
        ) : (
          <p className="text-fog">
            <span aria-hidden="true">✕ </span>skipped
          </p>
        )}
      </div>
    );
  }

  const answers = entry.questions.map((question, index) =>
    value(question, drafts[index] ?? blank())
  );
  const given = answers.filter((answer): answer is string => answer !== null);
  const ready = given.length === answers.length;

  const change = (index: number, next: Draft) =>
    setDrafts((all) => all.map((draft, at) => (at === index ? next : draft)));

  const send = () => {
    if (ready && armed(since.current, performance.now())) {
      onReply(given);
    }
  };

  const skip = () => {
    if (armed(since.current, performance.now())) {
      onReply(null);
    }
  };

  const keys = (event: KeyboardEvent<HTMLInputElement | HTMLButtonElement>) => {
    if (event.repeat) {
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      skip();
      return;
    }
    const { target } = event;
    if (
      event.key === "Enter" &&
      target instanceof HTMLInputElement &&
      (target.type === "radio" || target.type === "checkbox")
    ) {
      event.preventDefault();
      send();
    }
  };

  return (
    <form
      aria-label={`${agent} asks`}
      className="border-rim bg-raise ml-[2ch] flex flex-col gap-4 rounded-xl border p-3.5"
      onSubmit={(event) => {
        event.preventDefault();
        send();
      }}
    >
      <h2 className="text-paper font-semibold">{`${agent} asks`}</h2>
      {entry.questions.map((question, index) => {
        const draft = drafts[index] ?? blank();
        const group = `${entry.ask}-${index}`;
        return (
          <fieldset className="flex min-w-0 flex-col gap-2" key={group}>
            <legend className="text-paper mb-2">
              {question.header ? (
                <span className="text-fog block text-[12.5px]">
                  {question.header}
                </span>
              ) : null}
              {question.text}
            </legend>
            {[...question.choices, null].map((choice, at) => {
              const label = choice?.label ?? OTHER;
              const checked = choice
                ? draft.picked.includes(choice.label)
                : draft.other;
              return (
                <label
                  className="border-rim hover:border-fog active:bg-well has-[:checked]:border-ember keys:has-[:focus-visible]:outline-paper keys:has-[:focus-visible]:outline keys:has-[:focus-visible]:outline-offset-2 flex min-h-11 cursor-pointer items-start gap-2.5 rounded-lg border px-3 py-2.5 transition-colors"
                  key={label}
                >
                  <input
                    checked={checked}
                    className="accent-ember mt-0.5 size-4 flex-none cursor-pointer focus-visible:outline-none"
                    name={group}
                    onKeyDown={keys}
                    onChange={() => {
                      if (choice) {
                        change(
                          index,
                          toggle(draft, choice.label, question.multiple)
                        );
                        return;
                      }
                      opened.current = draft.other ? null : index;
                      change(index, elsewise(draft, question.multiple));
                    }}
                    ref={index === 0 && at === 0 ? first : undefined}
                    type={question.multiple ? "checkbox" : "radio"}
                  />
                  <span className="flex min-w-0 flex-col [overflow-wrap:anywhere]">
                    <span className="text-paper">{label}</span>
                    {choice?.about ? (
                      <span className="text-fog text-[13px]">
                        {choice.about}
                      </span>
                    ) : null}
                  </span>
                </label>
              );
            })}
            {draft.other ? (
              <input
                aria-label="your answer"
                className="border-rim bg-ink text-paper placeholder:text-fog focus-visible:border-ember min-h-9 rounded-lg border px-3 outline-none"
                maxLength={500}
                onChange={(event) =>
                  change(index, { ...draft, typed: event.target.value })
                }
                onKeyDown={keys}
                placeholder="type your answer"
                ref={(node) => {
                  if (node) {
                    fields.current.set(index, node);
                  } else {
                    fields.current.delete(index);
                  }
                }}
                type="text"
                value={draft.typed}
              />
            ) : null}
          </fieldset>
        );
      })}
      <div className="flex flex-wrap gap-2">
        <button
          className="bg-ember text-ink focus-visible:outline-paper min-h-9 cursor-pointer rounded-lg px-3.5 font-semibold hover:brightness-110 active:brightness-95 disabled:cursor-default disabled:opacity-40 disabled:hover:brightness-100"
          disabled={!ready}
          onKeyDown={keys}
          type="submit"
        >
          send <span className="font-normal opacity-70">enter</span>
        </button>
        <button
          className="border-rim text-paper hover:border-fog active:bg-well min-h-9 cursor-pointer rounded-lg border px-3.5 text-left"
          onClick={skip}
          onKeyDown={keys}
          type="button"
        >
          skip <span className="text-fog">esc</span>
        </button>
      </div>
    </form>
  );
};
