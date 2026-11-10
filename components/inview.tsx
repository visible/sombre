"use client";

import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";

interface Props {
  children: ReactNode;
  className?: string;
}

export const Inview = ({ children, className = "" }: Props) => {
  const ref = useRef<HTMLDivElement>(null);
  const [on, setOn] = useState(false);

  useEffect(() => {
    const node = ref.current;
    if (!node) {
      return;
    }
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry?.isIntersecting) {
          setOn(true);
          observer.disconnect();
        }
      },
      { threshold: 0.45 }
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  return (
    <div
      aria-hidden="true"
      className={`art ${on ? "on" : ""} ${className}`}
      ref={ref}
    >
      {children}
    </div>
  );
};
