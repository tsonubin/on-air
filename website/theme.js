/*
 * Theme toggle for the on-air project site.
 * The default follows prefers-color-scheme (see styles.css); this only
 * overrides it with an explicit choice kept in localStorage.
 */
(() => {
  const root = document.documentElement;
  const button = document.getElementById("theme-toggle");
  if (!button) {
    return;
  }

  const systemDark = () => window.matchMedia("(prefers-color-scheme: dark)").matches;
  const isDark = () => (root.dataset.theme ? root.dataset.theme === "dark" : systemDark());

  const render = () => {
    const label = `Switch to ${isDark() ? "light" : "dark"} theme`;
    button.textContent = label;
    button.setAttribute("aria-label", label);
    button.setAttribute("aria-pressed", String(isDark()));
  };

  const stored = localStorage.getItem("on-air-theme");
  if (stored === "light" || stored === "dark") {
    root.dataset.theme = stored;
  }
  render();

  button.addEventListener("click", () => {
    root.dataset.theme = isDark() ? "light" : "dark";
    localStorage.setItem("on-air-theme", root.dataset.theme);
    render();
  });
})();
