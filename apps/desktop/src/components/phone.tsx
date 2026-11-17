import { useCallback, useEffect, useState } from "react";

import type { Paired, Phone as Device } from "../lib/api";
import { failure, forget, pair, phones, unpair } from "../lib/api";
import { Copy } from "./copy";
import { secondary } from "./field";
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
      <div className="mt-4 flex flex-col gap-4 text-[12.5px]">
        {paired ? (
          <>
            <div className="flex items-start gap-4">
              <img
                alt="pairing code for this chat"
                className="size-40 shrink-0 rounded-lg"
                src={`data:image/svg+xml;utf8,${encodeURIComponent(paired.code)}`}
              />
              <div className="flex min-w-0 flex-col gap-2">
                <p className="text-paper">
                  scan this with your phone&apos;s camera
                </p>
                <p className="text-fog">
                  it opens this chat in your phone&apos;s browser,{" "}
                  {paired.reach}. the link holds the key and works once.
                </p>
                <div className="bg-ink flex min-w-0 items-center gap-1 rounded-lg py-1 pr-1 pl-2.5">
                  <span className="text-body min-w-0 flex-1 truncate">
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
          <output className="text-fog">making a link for this chat</output>
        ) : null}
        {problem ? (
          <div className="flex flex-col items-start gap-2" role="alert">
            <p className="text-paper">{problem}</p>
            <div className="flex flex-wrap gap-2">
              <button
                className={secondary}
                onClick={() => link()}
                type="button"
              >
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
        <div className="border-rim flex flex-col gap-1 border-t pt-3">
          <p className="text-fog">paired phones</p>
          {devices.length === 0 ? (
            <p className="text-fog">none yet</p>
          ) : (
            devices.map((device) => (
              <div className="flex items-center gap-2" key={device.id}>
                <span className="text-paper min-w-0 flex-1 truncate">
                  {device.label}
                  <span className="text-fog">{`  paired ${ago(device.since)}`}</span>
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
        </div>
      </div>
    </Sheet>
  );
};

export const Phone = ({ open, ...rest }: Props & { open: boolean }) =>
  open ? <Pairing {...rest} /> : null;
