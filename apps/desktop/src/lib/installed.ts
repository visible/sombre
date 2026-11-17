import { useEffect, useState } from "react";

import type { Installed } from "./api";
import { installs } from "./api";

const complete = (listed: Installed[] | null) =>
  listed !== null && listed.every((each) => each.version !== null);

export const useInstalled = () => {
  const [found, setFound] = useState<Installed[] | null>(null);
  useEffect(() => {
    let alive = true;
    let latest: Installed[] | null = null;
    const read = async () => {
      const listed = await installs().catch(() => null);
      latest = listed;
      if (alive) {
        setFound(listed);
      }
    };
    const again = () => {
      if (!complete(latest)) {
        read();
      }
    };
    read();
    window.addEventListener("focus", again);
    return () => {
      alive = false;
      window.removeEventListener("focus", again);
    };
  }, []);
  return found;
};
