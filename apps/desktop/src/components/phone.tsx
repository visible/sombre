import { useCallback, useEffect, useState } from "react";

import type { Paired, Phone as Device } from "../lib/api";
import { failure, forget, pair, phones, unpair } from "../lib/api";
import { Copy } from "./copy";
import { primary, secondary } from "./field";
import { Sheet } from "./sheet";

interface Props {
  chatKey: string;
  onClose: () => void;
}

const ago = (since: number) => {
  const minutes = Math.max(0, Math.round((Date.now() / 1000 - since) / 60));
  if (minutes < 1) {
    return "just now";
  }
  if (minutes < 60) {
    return `${minutes}m ago`;
  }
  const hours = Math.round(minutes / 60);
  return hours < 24 ? `${hours}h ago` : `${Math.round(hours / 24)}d ago`;
};

const Pairing = ({ chatKey, onClose }: Props) => {
  const [paired, setPaired] = useState<Paired | null>(null);
  const [devices, setDevices] = useState<Device[]>([]);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(true);

  const refresh = useCallback(async () => {
    setDevices(await phones().catch(() => []));
  }, []);

  const made = useCallback(
    async (home?: boolean) => {
      const result = await pair(chatKey, home).catch((error: unknown) => {
        setProblem(failure(error));
        return null;
      });
      if (result) {
        setPaired(result);
        setProblem(null);
        await refresh();
      }
      setBusy(false);
    },
    [chatKey, refresh]
  );

  const link = (home?: boolean) => {
    setBusy(true);
    setProblem(null);
    made(home);
  };

  useEffect(() => {
    const timer = window.setInterval(refresh, 3000);
    const first = window.setTimeout(() => made(), 0);
    return () => {
      window.clearInterval(timer);
      window.clearTimeout(first);
    };
  }, [made, refresh]);

  const stop = async () => {
    setBusy(true);
    await unpair().catch(() => false);
    setBusy(false);
    setPaired(null);
    setDevices([]);
    onClose();
  };

  return (
    <Sheet busy={false} onClose={onClose} open title="pair a phone">
      <p className="text-fog font-sans text-[13px]">
        continue this chat from your phone&apos;s browser
      </p>
      <div className="mt-4 flex flex-col gap-4 text-[12.5px]">
        {paired ? (
          <>
            <div className="flex items-start gap-4">
              <img
                alt="pairing code for this chat"
                className="size-40 shrink-0 rounded-lg"
                src={`data:image/svg+xml;utf8,${encodeURIComponent(paired.code)}`}
              />
              <div className="flex min-w-0 flex-col gap-2 pt-0.5">
                <p className="text-paper font-sans text-[13.5px] font-medium">
                  scan this with your phone&apos;s camera
                </p>
                <p className="text-fog font-sans text-[13px] leading-[1.55]">
                  it opens this chat {paired.reach}. the link holds the key and
                  works once.
                </p>
                <div className="bg-ink border-rim mt-1 flex min-w-0 items-center gap-1 rounded-lg border py-1 pr-1 pl-2.5">
                  <span className="text-body min-w-0 flex-1 truncate text-[12px]">
                    {paired.link}
                  </span>
                  <Copy text={paired.link} />
                </div>
              </div>
            </div>
            <div className="flex flex-wrap gap-2">
              <button
                className={secondary}
                disabled={busy}
                onClick={() => link(!paired.home)}
                type="button"
              >
                {paired.home ? "anywhere instead" : "home network instead"}
              </button>
              <button
                className={secondary}
                disabled={busy}
                onClick={stop}
                type="button"
              >
                stop pairing
              </button>
            </div>
          </>
        ) : null}
        {!paired && busy ? (
          <output className="flex items-start gap-4">
            <span className="bg-well size-40 shrink-0 rounded-lg motion-safe:animate-pulse" />
            <span className="text-fog pt-0.5 font-sans text-[13px]">
              making a link for this chat
            </span>
          </output>
        ) : null}
        {problem ? (
          <div
            className="bg-ink border-rim flex flex-col items-start gap-3 rounded-lg border p-3.5"
            role="alert"
          >
            <p className="text-paper font-sans text-[13px] leading-[1.55]">
              {problem}
            </p>
            <div className="flex flex-wrap gap-2">
              <button className={primary} onClick={() => link()} type="button">
                try again
              </button>
              <button
                className={secondary}
                onClick={() => link(true)}
                type="button"
              >
                home network instead
              </button>
            </div>
          </div>
        ) : null}
        <section className="border-rim flex flex-col gap-2 border-t pt-4">
          <h3 className="text-fog font-sans text-[12.5px]">paired phones</h3>
          {devices.length === 0 ? (
            <p className="text-fog font-sans text-[13px]">
              none yet. phones you pair show up here
            </p>
          ) : (
            devices.map((device) => (
              <div className="flex items-center gap-2.5" key={device.id}>
                <svg
                  aria-hidden="true"
                  className="text-fog size-4 shrink-0"
                  fill="none"
                  stroke="currentColor"
                  strokeLinecap="round"
                  strokeWidth="1.3"
                  viewBox="0 0 16 16"
                >
                  <rect height="13" rx="2" width="8" x="4" y="1.5" />
                  <path d="M7 12.5h2" />
                </svg>
                <span className="text-paper min-w-0 flex-1 truncate font-sans text-[13px]">
                  {device.label}
                  <span className="text-fog">{` · paired ${ago(device.since)}`}</span>
                </span>
                <button
                  className={secondary}
                  onClick={async () => {
                    await forget(device.id).catch(() => false);
                    await refresh();
                  }}
                  type="button"
                >
                  forget
                </button>
              </div>
            ))
          )}
        </section>
      </div>
    </Sheet>
  );
};

export const Phone = ({ open, ...rest }: Props & { open: boolean }) =>
  open ? <Pairing {...rest} /> : null;
