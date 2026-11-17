"use client";

import { useEffect } from "react";

const load = () => import("@/lib/reveal");

export const Reveal = () => {
  useEffect(() => {
    let undo: (() => void) | null = null;
    let live = true;
    const ready = async () => {
      await document.fonts.ready;
      const { reveal } = await load();
      if (live) {
        undo = reveal();
      }
    };
    void ready();
    return () => {
      live = false;
      undo?.();
    };
  }, []);
  return null;
};
