import { Icon, type IconName } from "@expo/ui";

/**
 * Builds an icon map whose entries resolve on first access. Each factory keeps
 * the literal `Icon.select({ ios, android: import(...) })` call so the Expo UI
 * Babel plugin can still fold it per platform; deferring the call means a
 * screen only loads the symbols it actually renders.
 */
function lazyIcons<K extends string>(factories: Record<K, () => IconName>): Record<K, IconName> {
  const cache = new Map<K, IconName>();
  const map = {} as Record<K, IconName>;
  for (const key of Object.keys(factories) as K[]) {
    Object.defineProperty(map, key, {
      enumerable: true,
      get() {
        let icon = cache.get(key);
        if (icon === undefined) {
          icon = factories[key]();
          cache.set(key, icon);
        }
        return icon;
      },
    });
  }
  return map;
}

export const icons = lazyIcons({
  monitor: () =>
    Icon.select({ ios: "display", android: import("@expo/material-symbols/monitor.xml") }),
  speaker: () =>
    Icon.select({ ios: "hifispeaker.fill", android: import("@expo/material-symbols/speaker.xml") }),
  chevron: () =>
    Icon.select({
      ios: "chevron.right",
      android: import("@expo/material-symbols/chevron_right.xml"),
    }),
  tune: () =>
    Icon.select({ ios: "slider.horizontal.3", android: import("@expo/material-symbols/tune.xml") }),
  more: () =>
    Icon.select({ ios: "ellipsis", android: import("@expo/material-symbols/more_horiz.xml") }),
  volume: () =>
    Icon.select({
      ios: "speaker.wave.2.fill",
      android: import("@expo/material-symbols/volume_up.xml"),
    }),
  live: () =>
    Icon.select({
      ios: "record.circle",
      android: import("@expo/material-symbols/radio_button_checked.xml"),
    }),
  route: () =>
    Icon.select({ ios: "arrow.down", android: import("@expo/material-symbols/south.xml") }),
  prev: () =>
    Icon.select({
      ios: "backward.end.fill",
      android: import("@expo/material-symbols/skip_previous.xml"),
    }),
  play: () =>
    Icon.select({ ios: "play.fill", android: import("@expo/material-symbols/play_arrow.xml") }),
  pause: () =>
    Icon.select({ ios: "pause.fill", android: import("@expo/material-symbols/pause.xml") }),
  next: () =>
    Icon.select({
      ios: "forward.end.fill",
      android: import("@expo/material-symbols/skip_next.xml"),
    }),
  computer: () =>
    Icon.select({ ios: "macbook", android: import("@expo/material-symbols/laptop_mac.xml") }),
  search: () =>
    Icon.select({ ios: "magnifyingglass", android: import("@expo/material-symbols/search.xml") }),
  lock: () => Icon.select({ ios: "lock.fill", android: import("@expo/material-symbols/lock.xml") }),
  check: () =>
    Icon.select({
      ios: "checkmark.circle.fill",
      android: import("@expo/material-symbols/check_circle.xml"),
    }),
});

export type AppIcon = keyof typeof icons;
