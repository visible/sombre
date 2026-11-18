import { openUrl } from "@tauri-apps/plugin-opener";
import { memo, useMemo } from "react";

import type { Block, Span } from "../lib/markdown";
import { markdown } from "../lib/markdown";
import { Copy } from "./copy";

const Inline = ({ spans }: { spans: Span[] }) =>
  spans.map((span) => {
    switch (span.kind) {
      case "rule": {
        return (
          <span className="border-rim my-2.5 block border-t" key={span.key} />
        );
      }
      case "break": {
        return <br key={span.key} />;
      }
      case "code": {
        return (
          <code
            className="bg-raise text-paper rounded-[3px] px-[3px] font-mono text-[13px]"
            key={span.key}
          >
            {span.text}
          </code>
        );
      }
      case "strong": {
        return (
          <strong className="text-paper font-semibold" key={span.key}>
            {span.text}
          </strong>
        );
      }
      case "em": {
        return (
          <em className="text-paper" key={span.key}>
            {span.text}
          </em>
        );
      }
      case "link": {
        return (
          <a
            className="text-paper decoration-fog hover:decoration-ember underline underline-offset-[3px]"
            href={span.href}
            key={span.key}
            onClick={(event) => {
              event.preventDefault();
              openUrl(span.href);
            }}
          >
            {span.text}
          </a>
        );
      }
      case "path": {
        return (
          <span className="text-paper" key={span.key} title={span.href}>
            {span.text}
          </span>
        );
      }
      default: {
        return span.text;
      }
    }
  });

const Part = ({ block }: { block: Block }) => {
  switch (block.kind) {
    case "heading": {
      return (
        <h3
          className={`text-paper font-semibold ${block.major ? "mt-5" : "mt-4"} first:mt-0`}
        >
          <Inline spans={block.spans} />
        </h3>
      );
    }
    case "rule": {
      return <hr className="border-rim my-2.5" />;
    }
    case "quote": {
      return (
        <blockquote className="border-rim text-fog border-l pl-3">
          <Inline spans={block.spans} />
        </blockquote>
      );
    }
    case "code": {
      return (
        <figure className="group/code relative flex flex-col gap-1">
          {block.language ? (
            <figcaption className="text-fog text-[12px]">
              {block.language}
            </figcaption>
          ) : null}
          <pre className="border-rim text-paper overflow-x-auto border-l py-0.5 pr-14 pl-3 font-mono text-[13px] leading-[1.6]">
            <code>{block.text}</code>
          </pre>
          <Copy
            className="absolute top-0 right-0 opacity-0 group-hover/code:opacity-100 focus-visible:opacity-100"
            text={block.text}
          />
        </figure>
      );
    }
    case "list": {
      const Tag = block.ordered ? "ol" : "ul";
      return (
        <Tag
          className={`flex flex-col gap-1 pl-[2ch] ${block.ordered ? "marker:text-fog list-decimal" : ""}`}
        >
          {block.items.map((item) => (
            <li
              className={
                block.ordered
                  ? ""
                  : "before:text-fog before:-ml-[2ch] before:inline-block before:w-[2ch] before:content-['•']"
              }
              key={item.key}
            >
              <Inline spans={item.spans} />
            </li>
          ))}
        </Tag>
      );
    }
    case "table": {
      const [head, ...body] = block.rows;
      return (
        <div className="border-rim overflow-x-auto rounded-lg border">
          <table className="w-full border-collapse text-[13px]">
            {head ? (
              <thead>
                <tr>
                  {head.cells.map((cell) => (
                    <th
                      className="border-rim text-paper border-b px-2.5 py-1.5 text-left align-top font-semibold"
                      key={cell.key}
                    >
                      <Inline spans={cell.spans} />
                    </th>
                  ))}
                </tr>
              </thead>
            ) : null}
            <tbody>
              {body.map((row) => (
                <tr className="border-rim border-b last:border-0" key={row.key}>
                  {row.cells.map((cell) => (
                    <td className="px-2.5 py-1.5 align-top" key={cell.key}>
                      <Inline spans={cell.spans} />
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    }
    default: {
      return (
        <p>
          <Inline spans={block.spans} />
        </p>
      );
    }
  }
};

interface Props {
  text: string;
}

const Prose = ({ text }: Props) => {
  const blocks = useMemo(() => markdown(text), [text]);
  return (
    <div className="flex flex-col gap-3 pl-[2ch] font-sans text-[15px] leading-[1.7] [overflow-wrap:anywhere]">
      {blocks.map((block) => (
        <Part block={block} key={block.key} />
      ))}
    </div>
  );
};

export const Markdown = memo(Prose);
