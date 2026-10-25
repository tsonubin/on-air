// The site remains readable with no JavaScript. Controls enhance static content.
const platformButtons = document.querySelector(".platform-buttons");
if (platformButtons) {
  const buttons = [...platformButtons.querySelectorAll("[data-platform]")];
  const panels = [...document.querySelectorAll("[data-install]")];
  for (const button of buttons) {
    button.addEventListener("click", () => {
      for (const peer of buttons) {
        peer.setAttribute("aria-pressed", String(peer === button));
      }
      for (const panel of panels) panel.hidden = panel.dataset.install !== button.dataset.platform;
    });
  }
  for (const panel of panels) panel.hidden = panel.dataset.install !== "linux";
  platformButtons.hidden = false;
}
for (const button of document.querySelectorAll(".copy-button")) {
  if (!navigator.clipboard?.writeText) continue;
  button.hidden = false;
  button.addEventListener("click", async () => {
    const command = button.closest(".command-block").querySelector("code").textContent;
    const status = document.querySelector(".copy-status");
    try {
      await navigator.clipboard.writeText(command);
      status.textContent = "Installation commands copied.";
    } catch {
      status.textContent = "Copy was unavailable. Select the commands and copy them manually.";
    }
  });
}
const storyControls = document.querySelector(".story-controls");
if (storyControls) {
  const buttons = [...storyControls.querySelectorAll("[data-view]")];
  for (const button of buttons) {
    button.addEventListener("click", () => {
      document.getElementById(`view-${button.dataset.view}`).scrollIntoView({
        behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches
          ? "instant"
          : "smooth",
        block: window.matchMedia("(min-width: 768px)").matches ? "center" : "nearest",
        inline: "center",
      });
    });
  }
  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        for (const button of buttons) {
          button.setAttribute(
            "aria-pressed",
            String(`view-${button.dataset.view}` === entry.target.id),
          );
        }
      }
    },
    { threshold: 0.6 },
  );
  for (const card of document.querySelectorAll(".story-card")) observer.observe(card);
  storyControls.hidden = false;
}
