import { Live, Window } from "@/components/art/frame";
import { Pills } from "@/components/art/pills";
import { Caret, Lines } from "@/components/art/terminal";
import type { Line } from "@/components/art/terminal";

const lines: Line[] = [
  { text: "sombre start", tone: "command" },
  { text: "Starting Claude Code in ~/code/atlas", tone: "output" },
  { aside: "pid 4182", text: "Agent running on this Mac", tone: "done" },
  { aside: "0 B sent", text: "Files stay on disk", tone: "done" },
  { aside: "iPhone", text: "Phone paired", tone: "done" },
  { text: "The relay only carries messages", tone: "output" },
];

const ghost = [
  { indent: 0, widths: [12, 6] },
  { indent: 2, widths: [8] },
  { indent: 2, widths: [14, 4] },
  { indent: 0, widths: [6] },
] as const;

export const Local = () => (
  <>
    <Window className="absolute top-16 -left-28 w-[190px] scale-95 opacity-60">
      <div className="px-4 py-4">
        <Pills rows={ghost} unit={3} />
      </div>
    </Window>
    <Window className="absolute top-16 -right-28 w-[190px] scale-95 opacity-60">
      <div className="px-4 py-4">
        <Pills rows={ghost} unit={3} />
      </div>
    </Window>
    <Window
      className="absolute top-6 left-1/2 w-[min(440px,88%)] -translate-x-1/2"
      name="atlas"
      title={<Live>Local only</Live>}
    >
      <div className="px-4 pt-3.5 pb-10">
        <Lines lines={lines} />
        <Caret at={lines.length} />
      </div>
    </Window>
  </>
);
