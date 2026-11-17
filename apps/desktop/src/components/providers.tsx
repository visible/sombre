import { useState } from "react";

import type { Preset, Provider, Settings } from "../lib/api";
import { connect, disconnect, rekey } from "../lib/api";
import { Row } from "./card";
import { Field, primary, secondary } from "./field";
import { Pick } from "./pick";
import { Remove } from "./remove";

export type Edit = (
  call: (place: string | null) => Promise<Settings>
) => Promise<string | null>;

const OTHER = "other";

const sourced = (key: string | null) => {
  if (key === null) {
    return "no key yet";
  }
  if (key === "saved key") {
    return "key saved in your config";
  }
  if (key === "keychain") {
    return "key in your keychain";
  }
  return `key from ${key}`;
};

const titled = (provider: Provider) =>
  provider.builtin ? "ai gateway" : provider.name;

const Problem = ({ text }: { text: string | null }) =>
  text ? (
    <p className="text-paper text-[12px]" role="alert">
      {text}
    </p>
  ) : null;

const Line = ({ provider, onEdit }: { provider: Provider; onEdit: Edit }) => {
  const [keying, setKeying] = useState(false);
  const [key, setKey] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const title = titled(provider);
  const kept = provider.key === "saved key" || provider.key === "keychain";

  const cancel = () => {
    setKeying(false);
    setKey("");
    setProblem(null);
  };

  const save = async () => {
    if (key.trim() === "") {
      return;
    }
    const failed = await onEdit((place) => rekey(provider.name, key, place));
    setProblem(failed);
    if (!failed) {
      cancel();
    }
  };

  const remove = async () => {
    setProblem(await onEdit((place) => disconnect(provider.name, place)));
  };

  return (
    <Row
      about={
        <>
          <span className="block">{provider.url}</span>
          <span className={`block ${provider.key ? "" : "text-paper"}`}>
            {sourced(provider.key)}
          </span>
        </>
      }
      below={
        keying || problem ? (
          <div className="flex flex-col gap-2">
            {keying ? (
              <div className="flex items-end gap-2">
                <Field
                  focus
                  label={`api key for ${title}`}
                  onCancel={cancel}
                  onChange={setKey}
                  onSubmit={save}
                  placeholder="paste the key. it goes in your keychain"
                  secret
                  value={key}
                />
                <button className={secondary} onClick={cancel} type="button">
                  cancel
                </button>
                <button
                  className={primary}
                  disabled={key.trim() === ""}
                  onClick={save}
                  type="button"
                >
                  save key
                </button>
              </div>
            ) : null}
            <Problem text={problem} />
          </div>
        ) : null
      }
      title={title}
    >
      <span className="flex shrink-0 items-center gap-1.5">
        <button
          aria-expanded={keying}
          className={secondary}
          onClick={() => (keying ? cancel() : setKeying(true))}
          type="button"
        >
          {provider.key ? "replace key" : "add key"}
        </button>
        {provider.builtin && !kept ? null : (
          <Remove
            confirm={provider.builtin ? "forget it" : `remove ${title}`}
            label={provider.builtin ? "forget key" : "remove"}
            onRemove={remove}
          />
        )}
      </span>
    </Row>
  );
};

const Connect = ({ presets, onEdit }: { presets: Preset[]; onEdit: Edit }) => {
  const [first] = presets;
  const [open, setOpen] = useState(false);
  const [kind, setKind] = useState(first?.id ?? OTHER);
  const [name, setName] = useState(first?.id ?? "");
  const [url, setUrl] = useState(first?.url ?? "");
  const [key, setKey] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const preset = presets.find((entry) => entry.id === kind) ?? null;

  const choose = (id: string) => {
    const found = presets.find((entry) => entry.id === id);
    setKind(id);
    setName(found?.id ?? "");
    setUrl(found?.url ?? "");
    setProblem(null);
  };

  const cancel = () => {
    setOpen(false);
    choose(first?.id ?? OTHER);
    setKey("");
  };

  const save = async () => {
    if (name.trim() === "" || url.trim() === "") {
      return;
    }
    const failed = await onEdit((place) =>
      connect({ key: preset?.local ? "" : key, name, url }, place)
    );
    setProblem(failed);
    if (!failed) {
      cancel();
    }
  };

  if (!open) {
    return (
      <Row
        about="ollama, lm studio, openai, anthropic, openrouter, groq, or any openai-compatible url"
        title="add a provider"
      >
        <button
          className={secondary}
          onClick={() => setOpen(true)}
          type="button"
        >
          add
        </button>
      </Row>
    );
  }

  return (
    <Row
      about="sombre's agent can use its models once it's added"
      below={
        <div className="flex flex-col gap-3">
          <div className="flex gap-2">
            <Field
              focus
              label="name"
              onCancel={cancel}
              onChange={setName}
              onSubmit={save}
              placeholder="lowercase, like my-llm"
              value={name}
            />
            <Field
              label="base url"
              onCancel={cancel}
              onChange={setUrl}
              onSubmit={save}
              placeholder="https://api.example.com/v1"
              value={url}
            />
          </div>
          {preset?.local ? (
            <p className="text-fog text-[12px]">
              runs on your computer, so it needs no key
            </p>
          ) : (
            <Field
              label="api key"
              onCancel={cancel}
              onChange={setKey}
              onSubmit={save}
              placeholder="paste the key. it goes in your keychain"
              secret
              value={key}
            />
          )}
          <Problem text={problem} />
          <div className="flex justify-end gap-2">
            <button className={secondary} onClick={cancel} type="button">
              cancel
            </button>
            <button
              className={primary}
              disabled={name.trim() === "" || url.trim() === ""}
              onClick={save}
              type="button"
            >
              add provider
            </button>
          </div>
        </div>
      }
      title="add a provider"
    >
      <Pick<string>
        label="what are you connecting?"
        onPick={choose}
        options={[
          ...presets.map((entry) => ({
            hint: entry.url,
            label: entry.label,
            value: entry.id,
          })),
          {
            hint: "any openai-compatible endpoint",
            label: "something else",
            value: OTHER,
          },
        ]}
        value={kind}
      />
    </Row>
  );
};

interface Props {
  providers: Provider[];
  presets: Preset[];
  onEdit: Edit;
}

export const Providers = ({ providers, presets, onEdit }: Props) => (
  <>
    {providers.map((provider) => (
      <Line key={provider.name} onEdit={onEdit} provider={provider} />
    ))}
    <Connect onEdit={onEdit} presets={presets} />
  </>
);
