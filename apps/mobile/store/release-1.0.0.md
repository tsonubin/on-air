# iOS 1.0.0 (8)

Release preparation on October 25, 2026.

- Integrated current mobile redesign and compatible maintenance into `main` on top of `origin/main`.
- Merged PRs #20, #27, #31, and #35. Closed stale PRs #11, #12, #13, #19, #21, #29, #30, and #32. No open PRs remained at verification.
- Retained the settings-save retry and bounded Linux Bluetooth discovery fixes from #29 without reviving the abandoned core integration stack.
- Fixed pnpm build-script approvals after CI and the first cloud build exposed invalid placeholder values.

## Build and validation

- [EAS build 8](https://expo.dev/accounts/tsonubin/projects/on-air-remote/builds/c8234a31-04c3-4693-bfcf-73f29ac3a069): finished.
- [Apple upload](https://expo.dev/accounts/tsonubin/projects/on-air-remote/submissions/d4601eef-66dc-4119-8dba-a7ad5d873976): finished. Apple reported `VALID`, internal `IN_BETA_TESTING`.
- TestFlight build 8 has test guidance saved and is assigned to the existing internal Team (Expo) group.
- [CI for f43db96](https://github.com/tsonubin/on-air/actions/runs/40142859414): all ten jobs passed, including native iOS/Android builds, Rust checks on three operating systems, and desktop/mobile end-to-end tests.
- Local checks passed: 224 mobile tests, 73 control-client tests, web typecheck, seven native-project checks, three focused settings tests, Rust formatting, and diff whitespace checks.
- Screenshots use the Release app in iPhone and iPad simulators with the desktop mock core. Physical speaker routing was not validated in this release session.

## Store and website preparation

The English copy and direct simulator screenshots are committed alongside this record. App Store Connect has build 8, iPhone/iPad screenshots, description, keywords, support/marketing URLs, companion setup instructions, and manual release configured. The Developer Program agreement was verified accepted on October 25.

Marketing, support, and privacy pages are now in `website/public`, with Vercel and Cloudflare deployment settings in `website/README.md`. The website was deployed with Wrangler to https://on-air.tsonubin.com on October 25. The custom domain and HTTPS are active, and the homepage, support, privacy, scripts, fonts, images and custom 404 were verified. `en-US.json` now uses this public origin; the App Store Connect draft still needs its previously proposed Sites URLs replaced. Cloudflare Worker version: `2837eea9-38e8-48b9-8154-04f7cb19ef77`.

The account holder confirmed free pricing. A zero-price schedule was configured in App Store Connect for all 175 price territories on October 25. This price schedule does not itself configure territorial app availability.

App Review submission remains pending final App Store URL updates, territorial availability, and the account holder's approval of Apple content/privacy/age declarations and private review contact. Do not interpret the prepared App Store draft as a submitted release.
