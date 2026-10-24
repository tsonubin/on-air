#!/usr/bin/env node
// Writes the faceplate's grain tile (assets/textures/grain.png, @2x, @3x): an
// 8-bit grayscale noise PNG with one noise pixel per device pixel, so the
// grain stays fine on 3x screens. The seed is fixed, so the output is
// reproducible. Run from apps/mobile: `node scripts/make-grain.mjs`.
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { crc32, deflateSync } from "node:zlib";

const TILE_PT = 128;
const SEED = 0x0a1a;
const outDir = join(dirname(fileURLToPath(import.meta.url)), "..", "assets", "textures");

function mulberry32(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, crc]);
}

function grainPng(size) {
  const rand = mulberry32(SEED);
  // Each row starts with filter byte 0 (None), then one gray byte per pixel.
  const raw = Buffer.alloc((size + 1) * size);
  for (let y = 0; y < size; y += 1) {
    const row = y * (size + 1);
    raw[row] = 0;
    for (let x = 0; x < size; x += 1) raw[row + 1 + x] = Math.floor(rand() * 256);
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(size, 0);
  header.writeUInt32BE(size, 4);
  header[8] = 8; // bit depth
  header[9] = 0; // color type: grayscale
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

mkdirSync(outDir, { recursive: true });
for (const [suffix, scale] of [
  ["", 1],
  ["@2x", 2],
  ["@3x", 3],
]) {
  writeFileSync(join(outDir, `grain${suffix}.png`), grainPng(TILE_PT * scale));
}
