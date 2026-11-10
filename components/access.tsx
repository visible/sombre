import { access } from "@/lib/site";

interface Props {
  size?: "small" | "large";
}

const sizes = {
  large: "h-11 px-5 text-[15px]",
  small: "h-9 px-4 text-[13px]",
} as const;

export const Access = ({ size = "large" }: Props) => (
  <a
    className={`bg-ink text-sheet hover:bg-ink/85 active:bg-ink inline-flex items-center justify-center rounded-full font-medium transition-colors ${sizes[size]}`}
    href={access}
  >
    Get early access
  </a>
);
