import { rays } from "@/lib/rays";

interface Props {
  className?: string;
  count?: number;
  disc?: number;
  filled?: boolean;
}

const width = 800;
const height = 400;

export const Sunset = ({
  className,
  count = 72,
  disc = 44,
  filled = true,
}: Props) => {
  const lines = rays({
    count,
    height,
    inner: filled ? disc + 18 : disc,
    jitter: 0.42,
    outer: 390,
    width,
  });
  return (
    <svg
      aria-hidden="true"
      className={className}
      fill="none"
      preserveAspectRatio="xMidYMax meet"
      viewBox={`0 0 ${width} ${height}`}
    >
      {lines.map((line) => (
        <line
          key={`${line.x2}-${line.y2}`}
          stroke="currentColor"
          strokeWidth="1.25"
          x1={line.x1}
          x2={line.x2}
          y1={line.y1}
          y2={line.y2}
        />
      ))}
      {filled ? (
        <path
          d={`M${width / 2 - disc} ${height}a${disc} ${disc} 0 0 1 ${disc * 2} 0Z`}
          fill="currentColor"
        />
      ) : null}
    </svg>
  );
};
