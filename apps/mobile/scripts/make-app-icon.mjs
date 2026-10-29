#!/usr/bin/env node
// Writes the Live Dial app icon from one set of shapes: the Icon Composer layers
// (assets/AppIcon.icon/Assets/dial-glyph.svg and on-air-dot.svg), a mirror of that
// package in the Xcode project, and the flat 1024 x 1024 fallback PNG (assets/icon.png
// and the Xcode marketing icon). icon.json is not generated: edit it in Icon Composer
// (or by hand) in assets/AppIcon.icon, then rerun this script to copy it into the
// Xcode project. Every run replaces the Xcode copy, so never edit that one directly.
// Run from apps/mobile: `node scripts/make-app-icon.mjs`. The native-project tests
// import `layers` and `flatPixels` to check the committed files are current.
import { cpSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { crc32, deflateSync } from "node:zlib";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const iconPackage = join(root, "assets", "AppIcon.icon");
const xcodePackage = join(root, "ios", "OnAirMobile", "AppIcon.icon");
const fallbacks = [
  join(root, "assets", "icon.png"),
  join(root, "ios", "OnAirMobile", "Images.xcassets", "AppIcon.appiconset", "AppIcon.png"),
];

// Geometry in points on the 1024 pt icon canvas, traced from the original raster.
const CANVAS = 1024;
const CHARCOAL = [21, 19, 16];
const IVORY = [243, 234, 212];
const RED = [255, 59, 42];
const BOWL = { cx: 485, cy: 560.5, outer: 271.5, inner: 162 };
const STEM = { left: 687.5, right: 797.5, top: 334.5, bottom: 830, radius: 37 };
// The dial's pointer: a radial cut through the bowl, clockwise from 12 o'clock.
const NOTCH = { degrees: 26.5, width: 30 };
const DOT = { cx: 744.5, cy: 250, r: 62.5 };

const rad = (degrees) => (degrees * Math.PI) / 180;
const deg = (radians) => (radians * 180) / Math.PI;
const hex = (rgb) => `#${rgb.map((c) => c.toString(16).padStart(2, "0")).join("")}`.toUpperCase();
const n = (value) => Number(value.toFixed(2));
const onBowl = (r, degrees) =>
  `${n(BOWL.cx + r * Math.sin(rad(degrees)))},${n(BOWL.cy - r * Math.cos(rad(degrees)))}`;

function glyphPath() {
  const half = NOTCH.width / 2;
  const outerCut = deg(Math.asin(half / BOWL.outer));
  const innerCut = deg(Math.asin(half / BOWL.inner));
  const stemMeetsBowl = deg(Math.asin((STEM.left - BOWL.cx) / BOWL.outer));
  const { left, right, top, bottom, radius } = STEM;
  const R = `${BOWL.outer} ${BOWL.outer} 0`;
  const r = `${BOWL.inner} ${BOWL.inner} 0`;
  const corner = `${radius} ${radius} 0 0 1`;
  return [
    // Outer bowl from the notch's clockwise side to where it meets the stem.
    `M${onBowl(BOWL.outer, NOTCH.degrees + outerCut)}`,
    `A${R} 0 1 ${onBowl(BOWL.outer, stemMeetsBowl)}`,
    // Stem: square top, rounded foot.
    `L${left},${top}H${right}V${bottom - radius}`,
    `A${corner} ${right - radius},${bottom}H${left + radius}`,
    `A${corner} ${left},${bottom - radius}`,
    // Back onto the bowl below the stem, round the long way to the notch.
    `L${onBowl(BOWL.outer, 180 - stemMeetsBowl)}`,
    `A${R} 1 1 ${onBowl(BOWL.outer, NOTCH.degrees - outerCut)}`,
    // In through the notch and round the counter.
    `L${onBowl(BOWL.inner, NOTCH.degrees - innerCut)}`,
    `A${r} 1 0 ${onBowl(BOWL.inner, NOTCH.degrees + innerCut)}Z`,
  ].join("");
}

const svg = (body) =>
  `<svg xmlns="http://www.w3.org/2000/svg" width="${CANVAS}" height="${CANVAS}" viewBox="0 0 ${CANVAS} ${CANVAS}">\n  ${body}\n</svg>\n`;

// The Icon Composer layers, by file name.
export const layers = {
  "dial-glyph.svg": svg(`<path fill="${hex(IVORY)}" d="${glyphPath()}"/>`),
  "on-air-dot.svg": svg(`<circle fill="${hex(RED)}" cx="${DOT.cx}" cy="${DOT.cy}" r="${DOT.r}"/>`),
};

// Point-in-shape tests mirroring glyphPath(), for the flat raster.
function inGlyph(x, y) {
  const dx = x - BOWL.cx;
  const dy = y - BOWL.cy;
  const d = Math.hypot(dx, dy);
  const a = rad(NOTCH.degrees);
  // Distance along the notch's radius, and across it.
  const along = dx * Math.sin(a) - dy * Math.cos(a);
  const across = dx * Math.cos(a) + dy * Math.sin(a);
  if (along > 0 && Math.abs(across) < NOTCH.width / 2) return false;
  if (d <= BOWL.outer && d >= BOWL.inner) return true;
  const { left, right, top, bottom, radius } = STEM;
  if (x < left || x > right || y < top || y > bottom) return false;
  if (y <= bottom - radius) return true;
  const cx = Math.min(Math.max(x, left + radius), right - radius);
  return Math.hypot(x - cx, y - (bottom - radius)) <= radius;
}
const inDot = (x, y) => Math.hypot(x - DOT.cx, y - DOT.cy) <= DOT.r;

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, crc]);
}

// The flat icon as PNG scanlines: a filter byte (0, None), then RGB for each pixel.
export function flatPixels() {
  const samples = 6;
  const row = CANVAS * 3 + 1;
  const raw = Buffer.alloc(row * CANVAS);
  for (let y = 0; y < CANVAS; y += 1) {
    raw[y * row] = 0; // filter: None
    for (let x = 0; x < CANVAS; x += 1) {
      let glyph = 0;
      let dot = 0;
      for (let sy = 0; sy < samples; sy += 1) {
        for (let sx = 0; sx < samples; sx += 1) {
          const px = x + (sx + 0.5) / samples;
          const py = y + (sy + 0.5) / samples;
          if (inGlyph(px, py)) glyph += 1;
          else if (inDot(px, py)) dot += 1;
        }
      }
      glyph /= samples * samples;
      dot /= samples * samples;
      for (let c = 0; c < 3; c += 1) {
        const value = CHARCOAL[c] * (1 - glyph - dot) + IVORY[c] * glyph + RED[c] * dot;
        raw[y * row + 1 + x * 3 + c] = Math.round(value);
      }
    }
  }
  return raw;
}

function png(raw) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(CANVAS, 0);
  header.writeUInt32BE(CANVAS, 4);
  header[8] = 8; // bit depth
  header[9] = 2; // color type: RGB
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  mkdirSync(join(iconPackage, "Assets"), { recursive: true });
  for (const [name, source] of Object.entries(layers)) {
    writeFileSync(join(iconPackage, "Assets", name), source);
  }
  rmSync(xcodePackage, { recursive: true, force: true });
  // Leave Finder's .DS_Store and other dotfiles behind.
  const visible = (source) => !basename(source).startsWith(".");
  cpSync(iconPackage, xcodePackage, { recursive: true, filter: visible });
  const icon = png(flatPixels());
  for (const file of fallbacks) writeFileSync(file, icon);
}
