const GLYPHS = "░▒▓<>/\\|=+*#%01";
const LINES = ["waking the sun", "sealing the channel", "you can step away"];
const WIDTH = 24;
const GAP = 400;
const LAND = 1480;
const root = document.documentElement;
const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
const boot = document.querySelector(".boot");
const said = document.querySelector("#said");
const sealed = document.querySelector("#sealed");

const scramble = () => GLYPHS[Math.floor(Math.random() * GLYPHS.length)];

const parts = (element) => {
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
  const found = [];
  while (walker.nextNode()) {
    found.push({
      node: walker.currentNode,
      text: walker.currentNode.nodeValue,
    });
  }
  return found;
};

const cipher = (element, level, ordered) => {
  const pieces = parts(element);
  const total = pieces.map((piece) => piece.text).join("").length;
  return {
    frame: 0,
    level,
    pieces,
    settle: Array.from({ length: total }, (_, index) =>
      ordered ? 1 - (index + 1) / (total + 1) : Math.random()
    ),
  };
};

const render = (state) => {
  let offset = 0;
  for (const piece of state.pieces) {
    const base = offset;
    piece.node.nodeValue = [...piece.text]
      .map((letter, index) =>
        letter.trim() && state.level > state.settle[base + index]
          ? scramble()
          : letter
      )
      .join("");
    offset += piece.text.length;
  }
};

const veil = (state, target, duration) => {
  cancelAnimationFrame(state.frame);
  const from = state.level;
  const start = performance.now();
  const step = (now) => {
    const progress = Math.min(1, (now - start) / duration);
    state.level = from + (target - from) * progress;
    render(state);
    if (progress < 1) {
      state.frame = requestAnimationFrame(step);
    }
  };
  state.frame = requestAnimationFrame(step);
};

const row = (label, time) => {
  const line = document.createElement("div");
  if (time < 0) {
    return line;
  }
  const full = `> ${label} ${".".repeat(WIDTH - label.length)} `;
  const typed = Math.min(full.length, Math.floor(time / 9));
  const ahead = Math.min(2, full.length - typed);
  line.textContent =
    full.slice(0, typed) + Array.from({ length: ahead }, scramble).join("");
  if (time > 380) {
    const ok = document.createElement("span");
    ok.className = "ok";
    ok.textContent = "ok";
    line.append(ok);
  } else if (typed === full.length) {
    line.append("█");
  }
  return line;
};

if ("boot" in root.dataset) {
  const start = performance.now();
  const name = cipher(document.querySelector("#name"), 1, true);
  const mark = cipher(document.querySelector("#mark"), 1, false);
  render(name);
  render(mark);
  let frame = 0;
  const land = () => {
    cancelAnimationFrame(frame);
    removeEventListener("keydown", land);
    removeEventListener("pointerdown", land);
    boot.replaceChildren();
    delete root.dataset.boot;
    root.dataset.landed = "";
    veil(mark, 0, 640);
    veil(name, 0, 720);
  };
  const type = (now) => {
    const time = now - start;
    if (time >= LAND) {
      land();
      return;
    }
    boot.replaceChildren(
      ...LINES.map((label, index) => row(label, time - 120 - index * GAP))
    );
    frame = requestAnimationFrame(type);
  };
  frame = requestAnimationFrame(type);
  addEventListener("keydown", land);
  addEventListener("pointerdown", land);
}

if (!still) {
  const state = cipher(sealed, 0, false);
  sealed.parentElement.addEventListener("pointerenter", () =>
    veil(state, 1, 360)
  );
  sealed.parentElement.addEventListener("pointerleave", () =>
    veil(state, 0, 360)
  );
}

for (const button of document.querySelectorAll("[data-copy]")) {
  const label = button.textContent.trim();
  let timer = 0;
  button.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(button.dataset.copy);
      button.textContent = "[copied]";
      button.dataset.done = "";
      said.textContent = "copied to your clipboard";
    } catch {
      button.textContent = "[select it]";
      said.textContent = "couldn't copy. select the command and copy it";
    }
    clearTimeout(timer);
    timer = setTimeout(() => {
      button.textContent = label;
      delete button.dataset.done;
      said.textContent = "";
    }, 1600);
  });
}
