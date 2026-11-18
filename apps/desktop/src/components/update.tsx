import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

import { failure, ready, restart } from "../lib/api";
import { toast } from "../lib/toasts";

export const relaunch = async () => {
  try {
    await restart();
    return true;
  } catch (error) {
    toast({ body: failure(error), title: "sombre didn't restart yet" });
    return false;
  }
};

export const Update = () => {
  const [version, setVersion] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    ready()
      .then(setVersion)
      .catch(() => null);
    const going = listen<string>("updated", ({ payload }) =>
      setVersion(payload)
    );
    return () => {
      going.then((stop) => stop()).catch(() => null);
    };
  }, []);

  if (!version) {
    return null;
  }

  const go = async () => {
    setBusy(true);
    if (!(await relaunch())) {
      setBusy(false);
    }
  };

  return (
    <div className="border-rim shrink-0 border-t p-2">
      <button
        className="text-paper hover:bg-raise active:bg-well focus-visible:outline-paper flex h-8 w-full cursor-pointer items-center gap-2 rounded-lg px-2 text-left text-[13px] disabled:cursor-default disabled:opacity-50"
        disabled={busy}
        onClick={go}
        type="button"
      >
        <span aria-hidden="true" className="bg-ember size-2 rounded-full" />
        <span className="flex-1">restart to update</span>
        <span className="text-fog text-[11px]">{version}</span>
      </button>
    </div>
  );
};
