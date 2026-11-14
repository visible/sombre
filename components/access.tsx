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
    aria-label="Get early access"
    className={`pill bg-ink text-sheet inline-flex items-center justify-center rounded-full font-medium ${sizes[size]}`}
    href={access}
  >
    <span className="roll">
      <span>Get early access</span>
      <span aria-hidden="true">Get early access</span>
    </span>
  </a>
);
