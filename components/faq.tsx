import { Tag } from "@/components/tag";

const questions = [
  {
    answer:
      "Yes. The agent runs on your Mac, so it needs to be awake and online while you work from your phone.",
    question: "Does my Mac need to stay on?",
  },
  {
    answer:
      "No. Sessions open in your phone's browser. You can add Sombre to your home screen if you want it one tap away.",
    question: "Do I need to install anything on my phone?",
  },
  {
    answer: "Claude Code and Codex at launch, with more to follow.",
    question: "Which agents does it work with?",
  },
  {
    answer:
      "Sombre is in early development. Ask for early access and we'll email you when the first build is ready.",
    question: "When can I try it?",
  },
] as const;

export const Faq = () => (
  <section
    aria-labelledby="answers"
    className="border-line grid scroll-mt-4 gap-10 border-t px-5 py-16 sm:px-8 lg:grid-cols-12 lg:py-24"
    id="questions"
  >
    <div className="lg:col-span-5">
      <Tag>Questions</Tag>
      <h2
        className="text-ink mt-6 text-[clamp(2rem,3.6vw,3rem)] leading-[1.05] font-medium tracking-[-0.03em]"
        id="answers"
      >
        Good to know
      </h2>
    </div>
    <div className="group/faq lg:col-span-7 lg:pl-4">
      <div className="divide-line border-line divide-y border-y">
        {questions.map((item) => (
          <details className="group" key={item.question} name="faq">
            <summary className="text-ink flex cursor-pointer list-none items-center justify-between gap-6 py-5 text-[17px] font-medium [&::-webkit-details-marker]:hidden">
              {item.question}
              <span
                aria-hidden="true"
                className="bg-sheet group-open:bg-ink group-open:text-sheet flex size-8 shrink-0 items-center justify-center rounded-full transition-colors"
              >
                <svg
                  className="size-3.5 transition-transform duration-200 group-open:rotate-45"
                  fill="none"
                  viewBox="0 0 14 14"
                >
                  <path
                    d="M7 2.5v9M2.5 7h9"
                    stroke="currentColor"
                    strokeLinecap="round"
                    strokeWidth="1.5"
                  />
                </svg>
              </span>
            </summary>
            <p className="text-body min-h-[calc(3lh+1.5rem)] max-w-[58ch] pb-6 text-[15px] leading-relaxed sm:pr-14">
              {item.answer}
            </p>
          </details>
        ))}
      </div>
      <div
        aria-hidden="true"
        className="h-[calc(3lh+1.5rem)] text-[15px] leading-relaxed group-has-[details[open]]/faq:hidden"
        data-reserve=""
      />
    </div>
  </section>
);
