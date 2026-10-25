# on-air website

A standalone static product website. HTML, CSS, self-hosted GSAP/ScrollTrigger, Space Grotesk display type and Geist body typography, Phosphor icons, and real product screenshots. No package installation, compilation, server, environment variables, or analytics are required in production.

## Preview locally

From the repository root:

```sh
python3 -m http.server 48762 --bind 127.0.0.1 --directory website/public
```

Open <http://127.0.0.1:48762>. Edit `public/index.html`, `public/styles.css`, and `public/site.js` directly. The install selector, command-copy button, and screenshot carousel controls use JavaScript; all installation options and product screenshots remain available without it. The copy button needs a secure browser context (HTTPS or localhost).

## Deploy later

Only `website/public/` is the public artifact. Keep the project root at `website` so the host does not install or build the Rust/mobile monorepo.

| Setting | Vercel | Cloudflare Pages |
| --- | --- | --- |
| Repository | This repository | This repository |
| Root directory | `website` | `website` |
| Framework preset | Other | None |
| Build command | Empty (override enabled) | `exit 0` |
| Output directory | `public` | `public` |
| Install command | `true` (skip; set in vercel.json) | No dependencies to install |

`vercel.json` records the Vercel overrides. For Cloudflare Pages Git integration, enter the settings above in the dashboard. For Cloudflare Workers static assets, `cloudflare/wrangler.jsonc` also points at `public`; from `website/`, use the official Wrangler CLI's `wrangler deploy --config cloudflare/wrangler.jsonc` after selecting your account. No Worker script or bindings are needed. These instructions prepare deployment; no production deployment was performed in this change.

Official references: [Vercel static builds](https://vercel.com/docs/builds/configure-a-build#skip-build-step), [Cloudflare Pages static HTML](https://developers.cloudflare.com/pages/framework-guides/deploy-anything/), [Cloudflare Workers static assets](https://developers.cloudflare.com/workers/static-assets/get-started/).

## App Store handoff after hosting

The final domain is intentionally unspecified. Every internal link and asset path is relative, and there is no dependency on the earlier prepared Sites project.

After deployment, set the App Store URLs and `apps/mobile/store/en-US.json` to the real public origin:

- Marketing: `/`
- Support: `/support.html`
- Privacy policy: `/privacy.html`

Open each URL without authentication before submitting. The existing App Store draft still references the previously proposed Sites origin; it must be replaced. Add an absolute `og:image` and canonical URL after choosing a domain, if desired. Update the iOS release notice and add a real App Store link only after the app is publicly available.

## Content and assets

Pricing is free, with no paid tier or in-app purchases. Platform claims follow the repository's README and install guide; published desktop installers may trail the source. The site links to installation guidance rather than advertising unverified download assets.

The phone captures come from `apps/mobile/store/screenshots` and show the real Release app using demonstration devices. WebP derivatives are optimized copies; App Store PNG originals remain unchanged. `patchbay.png` is the actual Qt render from `apps/quickshell/preview.png`. Space Grotesk and Geist fonts are self-hosted under the included OFL license; Phosphor Regular icons retain their MIT license in `assets/icons/LICENSE`. The cinematic listening-room backdrop was generated for the site and is atmosphere artwork, not a photograph of on-air hardware. The product screenshots remain actual captures. The site uses a dark listening-room palette and a centered cinematic hero.

## Motion

`public/motion.js` uses self-hosted GSAP 3.15.0 and ScrollTrigger from the official npm package. Vendor files retain upstream license headers and are excluded from source formatting. GSAP has a separate [Standard License](https://gsap.com/standard-license/).

The design follows the requested GPT-Taste direction: cinematic centered hero, Space Grotesk display type with Geist body text, a filled six-column feature grid (3 + 3, then 6), inline imagery, an icon marquee, and screenshot carousel controls. ScrollTrigger pins the remote copy on desktop while the gallery advances beside it. The statement scrubs into view; the hero image and product panels move with native scroll. Mobile uses a native horizontal scroll-snap gallery. No wheel interception or smooth-scroll replacement is used.

`gsap.matchMedia` reverts pins and inline styles when reduced motion or viewport conditions change. Reduced motion removes scroll effects and marquee movement, and the marquee also has a pause control. No content is hidden in CSS awaiting JavaScript. If GSAP fails to load, the static page remains usable.
