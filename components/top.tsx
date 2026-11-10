"use client";

export const Top = () => (
  <button
    className="text-meta hover:text-ink order-2 flex cursor-pointer items-center gap-2 justify-self-start text-[13px] transition-colors md:order-none md:justify-self-end"
    onClick={() => {
      window.scrollTo({ top: 0 });
      document
        .querySelector<HTMLAnchorElement>("header a")
        ?.focus({ preventScroll: true });
    }}
    type="button"
  >
    Back to top
    <span
      aria-hidden="true"
      className="bg-ink text-sheet flex size-6 items-center justify-center rounded-full"
    >
      <svg className="size-3" fill="none" viewBox="0 0 12 12">
        <path
          d="M6 9.5v-7M3 5.5l3-3 3 3"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth="1.5"
        />
      </svg>
    </span>
  </button>
);
