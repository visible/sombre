interface Props {
  start: number;
}

const path = "M 470 22 H 680 C 742 22 772 44 790 76 C 802 98 808 108 808 122";

export const Handoff = ({ start }: Props) => (
  <div
    aria-hidden="true"
    className="pointer-events-none absolute inset-0 hidden lg:block"
  >
    <svg
      className="absolute inset-0 overflow-visible"
      fill="none"
      height="600"
      viewBox="0 0 940 600"
      width="940"
    >
      <defs>
        <filter height="200%" id="glow" width="200%" x="-50%" y="-50%">
          <feGaussianBlur stdDeviation="5" />
        </filter>
      </defs>
      <g className="groove" style={{ animationDelay: `${start - 200}ms` }}>
        <path
          d={path}
          stroke="rgb(15 16 17 / 0.14)"
          strokeLinecap="round"
          strokeWidth="18"
        />
        <path
          d={path}
          stroke="rgb(255 255 255 / 0.35)"
          strokeLinecap="round"
          strokeWidth="1"
          transform="translate(0 -6)"
        />
      </g>
      <path
        className="trail"
        d={path}
        filter="url(#glow)"
        pathLength="100"
        stroke="var(--color-sheet)"
        strokeLinecap="round"
        strokeWidth="8"
        style={{ animationDelay: `${start}ms` }}
      />
    </svg>
    <div
      className="travel absolute top-0 left-0 flex h-[34px] w-[58px] items-center justify-center rounded-full bg-white/75 shadow-[0_10px_24px_-8px_rgb(60_15_0/0.55),inset_0_1px_0_rgb(255_255_255/0.9)] ring-1 ring-white/70 backdrop-blur-md"
      style={{ animationDelay: `${start}ms`, offsetPath: `path("${path}")` }}
    >
      <span className="bg-ember/70 absolute right-2 size-4 rounded-full blur-[6px]" />
      <svg
        className="spin text-ink relative size-4"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="1.5"
        style={{ animationDelay: `${start}ms` }}
        viewBox="0 0 16 16"
      >
        <path d="M8 1.5v2.5M8 12v2.5M1.5 8H4M12 8h2.5M3.4 3.4l1.8 1.8M10.8 10.8l1.8 1.8M3.4 12.6l1.8-1.8M10.8 5.2l1.8-1.8" />
      </svg>
    </div>
  </div>
);
