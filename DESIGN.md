# Design

The brief, concept and system behind the `reimagine` branch. Read this before changing layout or motion so later passes stay consistent.

## Brief

- Subject: Sombre, a Mac app that runs Claude Code and Codex locally and lets a phone browser continue the same session.
- Audience: developers who already use coding agents.
- Job: explain the handoff in seconds and collect early access emails (`mailto:`).
- Constraints: same palette, type, hairline grid and sunset mark as the first landing page. Same stack (Next.js, Tailwind, Bun).

## Concept card

```
Idea:        The page is one session. Scroll off the desk and it leaves the Mac window and comes with you.
Metaphor:    A handoff from desk to pocket. The pocket is a capsule that stays with the visitor.
Signature:   The pocket session. It appears when the hero leaves the viewport, advances as sections
             pass, waits for the visitor to tap Approve, and steps aside when the footer arrives.
Emotion:     Calm confidence. You can walk away and nothing is lost.
Motion verb: Hand off. Things leave one side and dock on the other; labels roll, lines draw from the left.
Screenshot:  "Step away." across the full width above the ember field, or the capsule asking to run bun test.
```

Anchor: the tagline "Step away. The session comes with you." and the product's one mechanic, approving a command from your phone. Swap in another product and the capsule has nothing to say.

Concepts considered: the sun in the mark tracking a working day (too close to a time-of-day scroll trope, and it would change the palette), and a strict two-column "two screens" grid (a layout, not an idea). The footer keeps the first one as a detail: the capsule leaves as the sun rises.

## System

- Palette (unchanged): paper `#edefef` ground, sheet `#f7f8f8` surfaces, line `#d6dadb` hairlines, ink `#0f1011`, body `#4a4f51`, meta `#5e6466`, fog `#9aa0a3` (on ink only), ember `#ff5b14` accent and hero field.
- Type (unchanged families): Host Grotesk for display and text, IBM Plex Mono for times, paths and terminal lines. The hero line is sized from the container (`cqi`) so it fills the width at every breakpoint.
- Grid: 12 columns inside the bordered 1280 px frame. Text hangs from column 1 and column 6 on every page.
- Shape: pills for actions and the capsule, 12 to 16 px radii for windows, hairlines everywhere else.
- Motion: one ease family (expo out, `--ease`), GSAP ticker as the only clock, Lenis for scroll. Headings reveal by line, hairlines draw from the left, pages hand off sideways (out to the left, in from the right).
- Hover roles: primary pills roll their label; nav has a sliding ember marker; text links draw an underline; arrows nudge along their direction.
- Idle: the capsule's live dot breathes and the rays behind the hero and footer drift and lean to the pointer. All off under reduced motion.

## Driver

`lib/session.ts` owns the state (`live`, `peek`, `approved`, `running`). Sections declare `data-session="<event id>"`; `components/pocket.tsx` advances the session as they pass and renders the capsule. The session only moves forward, it stops at the approval until the visitor taps, and the replay in the history section previews events through `peek`. The store is module level, so the capsule keeps its state across pages.

## Sections (home)

```
Hero       poster: "Step away." full width, second line and copy on column 6, ember field with the demo
How        index rows: time and place on column 1, step on column 6, hairline draws per row
History    asymmetric pair: scrub list 5 cols, replay window 7 cols
Statement  one large paragraph on what travels and what stays
Why        bento 7/5 then 5/7 with the animated art
Questions  5/7 with details rows
Footer     call to action, link columns, horizon, email
```

## Decisions

- No loader and no WebGL: a product page with text as the LCP element does not earn them.
- The hero intro is CSS only, so the heading paints without waiting for JavaScript.
- The capsule floats over content like a phone's live activity. It sits before the footer in the DOM so Tab reaches Approve, and it is `inert` while hidden.
- The capsule reads element positions on the GSAP ticker instead of using ScrollTrigger, which missed the footer on touch viewports.
- Nothing is hidden in CSS before a reveal; GSAP sets the start state, so content is complete with scripts off and under reduced motion.
