const seen = () => {
  try {
    const before = sessionStorage.getItem("seen") !== null;
    sessionStorage.setItem("seen", "");
    return before;
  } catch {
    return false;
  }
};

if (seen()) {
  document.documentElement.dataset.seen = "";
} else if (!matchMedia("(prefers-reduced-motion: reduce)").matches) {
  document.documentElement.dataset.boot = "";
}
