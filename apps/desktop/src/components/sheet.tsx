import type { ReactNode } from "react";
import { useEffect, useRef } from "react";

interface Props {
  open: boolean;
  title: string;
  busy: boolean;
  children: ReactNode;
  onClose: () => void;
}

export const Sheet = ({ open, title, busy, children, onClose }: Props) => {
  const dialog = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const element = dialog.current;
    if (open && element && !element.open) {
      element.showModal();
    } else if (!open && element?.open) {
      element.close();
    }
  }, [open]);

  return (
    <dialog
      aria-label={title}
      className="border-rim bg-raise text-body backdrop:bg-ink/60 shadow-lift relative m-auto w-[min(460px,90vw)] rounded-2xl border p-5 outline-none focus-visible:outline-none"
      onCancel={(event) => {
        if (busy) {
          event.preventDefault();
        }
      }}
      onClose={onClose}
      ref={dialog}
    >
      <div className="mb-1 flex items-start justify-between gap-3">
        <h2 className="text-paper pt-0.5 font-sans text-[15px] font-medium">
          {title}
        </h2>
        <button
          aria-label="close"
          className="text-fog hover:bg-well hover:text-paper active:bg-rim -mt-1 -mr-1.5 flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-lg disabled:cursor-default disabled:opacity-40"
          disabled={busy}
          onClick={() => dialog.current?.close()}
          title="close (esc)"
          type="button"
        >
          <svg
            aria-hidden="true"
            className="size-3.5"
            fill="none"
            stroke="currentColor"
            strokeLinecap="round"
            strokeWidth="1.5"
            viewBox="0 0 16 16"
          >
            <path d="M4 4l8 8M12 4l-8 8" />
          </svg>
        </button>
      </div>
      {children}
    </dialog>
  );
};
