// Self-hosted GSAP. Native scroll, no scroll hijacking or wheel listeners.
const { gsap, ScrollTrigger } = window;
if (gsap && ScrollTrigger) {
  gsap.registerPlugin(ScrollTrigger);
  const media = gsap.matchMedia();
  let entered = false;
  media.add(
    {
      desktop: "(min-width: 768px) and (min-height: 650px)",
      motion: "(prefers-reduced-motion: no-preference)",
    },
    ({ conditions }) => {
      if (!conditions.motion) return;
      if (!entered) {
        gsap.from(".hero-copy > *", {
          y: 32,
          opacity: 0,
          stagger: 0.14,
          duration: 1.15,
          ease: "power3.out",
          clearProps: "all",
        });
        entered = true;
      }
      gsap.fromTo(
        ".hero-backdrop",
        { scale: 1.1 },
        {
          scale: 1,
          yPercent: 8,
          ease: "none",
          scrollTrigger: { trigger: ".hero", start: "top top", end: "bottom top", scrub: 0.8 },
        },
      );
      gsap.to(".hero-copy", {
        y: -65,
        opacity: 0.15,
        ease: "none",
        scrollTrigger: { trigger: ".hero", start: "20% top", end: "bottom 30%", scrub: 0.7 },
      });
      for (const card of gsap.utils.toArray(".feature-card")) {
        gsap.from(card, {
          y: 60,
          opacity: 0.25,
          duration: 1,
          ease: "power3.out",
          scrollTrigger: {
            trigger: card,
            start: "top 88%",
            toggleActions: "play none none reverse",
          },
        });
      }
      gsap.from(".reveal-word", {
        opacity: 0.12,
        stagger: 0.6,
        ease: "none",
        scrollTrigger: { trigger: ".statement", start: "top 72%", end: "bottom 48%", scrub: 0.8 },
      });
      if (conditions.desktop) {
        ScrollTrigger.create({
          trigger: ".experience-grid",
          start: "top top",
          end: "bottom bottom",
          pin: ".experience-copy",
          pinSpacing: false,
          invalidateOnRefresh: true,
        });
        for (const device of gsap.utils.toArray(".story-device")) {
          const timeline = gsap.timeline({
            scrollTrigger: { trigger: device, start: "top 95%", end: "bottom 5%", scrub: 0.7 },
          });
          timeline
            .fromTo(device, { scale: 0.84, opacity: 0.3 }, { scale: 1, opacity: 1, duration: 1 })
            .to(device, { scale: 0.96, opacity: 0.3, duration: 0.7 }, 1.4);
        }
      }
      gsap.from(".free-price", {
        y: 35,
        opacity: 0.2,
        duration: 1,
        ease: "power3.out",
        scrollTrigger: {
          trigger: ".download",
          start: "top 70%",
          toggleActions: "play none none reverse",
        },
      });
      const marquee = gsap.to(".transport-track", {
        xPercent: -50,
        duration: 30,
        repeat: -1,
        ease: "none",
      });
      const toggle = document.querySelector(".marquee-toggle");
      let paused = false;
      toggle.setAttribute("aria-pressed", "false");
      toggle.setAttribute("aria-label", "Pause connection marquee");
      toggle.firstElementChild.className = "icon icon-pause";
      const handler = () => {
        paused = !paused;
        marquee.paused(paused);
        toggle.setAttribute("aria-pressed", String(paused));
        toggle.setAttribute(
          "aria-label",
          paused ? "Play connection marquee" : "Pause connection marquee",
        );
        toggle.firstElementChild.className = `icon icon-${paused ? "play" : "pause"}`;
      };
      toggle.addEventListener("click", handler);
      toggle.hidden = false;
      ScrollTrigger.create({
        trigger: ".transport-strip",
        start: "top bottom",
        end: "bottom top",
        onEnter: () => {
          if (!paused) marquee.play();
        },
        onEnterBack: () => {
          if (!paused) marquee.play();
        },
        onLeave: () => marquee.pause(),
        onLeaveBack: () => marquee.pause(),
      });
      return () => {
        toggle.removeEventListener("click", handler);
        toggle.hidden = true;
      };
    },
  );
  document.fonts.ready.then(() => ScrollTrigger.refresh());
  window.addEventListener("load", () => ScrollTrigger.refresh(), { once: true });
  document
    .querySelector(".platform-buttons")
    ?.addEventListener("click", () => ScrollTrigger.refresh());
  document.querySelector(".commands")?.addEventListener("toggle", () => ScrollTrigger.refresh());
}
