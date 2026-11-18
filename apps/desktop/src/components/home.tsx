import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";

import type { Agent, Folder, Installed } from "../lib/api";
import { agents, failure, folder as describe, names } from "../lib/api";
import { Logo } from "./logo";
import { Mark } from "./mark";
import { Missing } from "./missing";
import { Divider, Option, Popover, Search, chip } from "./popover";

interface Props {
  agent: Agent;
  folder: Folder | null;
  folders: Folder[];
  missing: Installed | null;
  onAgent: (agent: Agent) => void;
  onFolder: (folder: Folder) => void;
}

export const Home = ({
  agent,
  folder,
  folders,
  missing,
  onAgent,
  onFolder,
}: Props) => {
  const [problem, setProblem] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const listed =
    folder && !folders.some((entry) => entry.path === folder.path)
      ? [folder, ...folders]
      : folders;
  const shown = listed.filter((entry) =>
    entry.place.toLowerCase().includes(query.toLowerCase().trim())
  );

  const browse = async () => {
    setProblem(null);
    try {
      const picked = await open({
        directory: true,
        title: "pick a folder for the new chat",
      });
      if (typeof picked === "string") {
        onFolder(await describe(picked));
      }
    } catch (error) {
      setProblem(failure(error));
    }
  };

  return (
    <div className="flex flex-col items-center gap-4 px-6 text-center">
      <Mark className="text-ember mb-1 h-6 w-10" />
      <h2 className="text-paper text-[22px] leading-snug">
        what should we work on?
      </h2>
      <div className="text-fog flex flex-wrap items-center justify-center gap-x-[1ch] gap-y-2">
        <span>in</span>
        <span className="whitespace-nowrap">
          <Popover
            align="start"
            className="text-paper decoration-edge hover:decoration-fog aria-expanded:decoration-ember cursor-pointer font-semibold underline decoration-dotted decoration-1 underline-offset-[6px]"
            label="pick a folder"
            side="bottom"
            trigger={folder?.project ?? "a folder"}
            width="w-80"
          >
            {(close) => (
              <>
                {listed.length > 6 ? (
                  <Search
                    onChange={setQuery}
                    placeholder="search folders"
                    value={query}
                  />
                ) : null}
                <div className="flex min-h-0 flex-col overflow-y-auto overscroll-contain">
                  {shown.map((entry) => (
                    <Option
                      hint={entry.place}
                      key={entry.path}
                      label={entry.project}
                      onPick={() => {
                        onFolder(entry);
                        close();
                      }}
                      selected={folder?.path === entry.path}
                    />
                  ))}
                </div>
                <Divider />
                <Option
                  icon="+"
                  label="choose another folder"
                  onPick={() => {
                    close();
                    browse();
                  }}
                />
              </>
            )}
          </Popover>
        </span>
      </div>
      <fieldset className="border-rim flex rounded-xl border p-0.5">
        <legend className="sr-only">agent</legend>
        {agents.map((option) => (
          <label
            className={`${chip} has-checked:bg-well has-checked:text-paper keys:has-[:focus-visible]:outline-paper keys:has-[:focus-visible]:outline keys:has-[:focus-visible]:outline-1 px-2.5`}
            key={option}
          >
            <input
              checked={agent === option}
              className="sr-only"
              name="agent"
              onChange={() => onAgent(option)}
              type="radio"
            />
            <Logo agent={option} />
            {names[option]}
          </label>
        ))}
      </fieldset>
      {missing ? <Missing entry={missing} /> : null}
      {problem ? <p className="text-fog">{problem}</p> : null}
      {folder ? <p className="text-fog text-[12px]">{folder.place}</p> : null}
    </div>
  );
};
