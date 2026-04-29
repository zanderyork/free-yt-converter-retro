#!/usr/bin/env node
/* gen-icon.js — generate a 1024×1024 PNG of the RetroTube app icon.
 *
 * The design is a chunky 32×32 pixel-art mark scaled 32× by nearest-neighbor
 * duplication, so it stays crisp at every Tauri-required icon size. The
 * subject is a retro cassette tape with two reels and a small red record dot,
 * framed in a navy/blue Win2K-style chrome border.
 *
 * Output: src-tauri/icons/icon-source.png (1024×1024 RGBA).
 *
 * Then: run `npx tauri icon src-tauri/icons/icon-source.png` to generate
 * the full Tauri icon set.
 */
"use strict";

const fs = require("fs");
const path = require("path");
const zlib = require("zlib");

const OUT = path.resolve(__dirname, "..", "src-tauri", "icons", "icon-source.png");
const SCALE = 32;
const W = 32;
const H = 32;

// --- color palette (RGBA tuples) -----------------------------------------
const C = {
  T: [0, 0, 0, 0],          // transparent
  K: [0, 0, 0, 255],        // black outline
  N: [10, 36, 106, 255],    // navy (Win2K title bar)
  L: [166, 202, 240, 255],  // light blue
  S: [212, 208, 200, 255],  // Win2K silver face
  W: [255, 255, 255, 255],  // white
  D: [64, 64, 64, 255],     // dark grey
  G: [128, 128, 128, 255],  // mid grey
  R: [192, 0, 0, 255],      // red record dot
  Y: [240, 220, 80, 255],   // tape (yellow-ish)
};

// --- 32x32 pixel grid -----------------------------------------------------
// Legend: . = transparent
//         K = black outline
//         N = navy frame
//         L = light blue inner frame
//         S = silver face
//         W = white highlight
//         D = dark grey
//         G = grey
//         R = red dot
//         Y = tape
//
// Rows are read top-to-bottom; columns left-to-right.

const GRID = [
  "..NNNNNNNNNNNNNNNNNNNNNNNNNNNN..",
  ".NWWWWWWWWWWWWWWWWWWWWWWWWWWWWN.",
  ".NWLLLLLLLLLLLLLLLLLLLLLLLLLLWN.",
  ".NWLSSSSSSSSSSSSSSSSSSSSSSSSLWN.",
  ".NWLSSSSSSSSSSSSSSSSSSSSSSSSLWN.",
  ".NWLSSKKKKKKKKKKKKKKKKKKKKSSLWN.",
  ".NWLSKWWWWWWWWWWWWWWWWWWWWKSLWN.",
  ".NWLSKWGGGGGGGGGGGGGGGGGGWKSLWN.",
  ".NWLSKWGKKKKKGGGGGGKKKKKGWKSLWN.",
  ".NWLSKWGKDDDKGGGGGGKDDDKGWKSLWN.",
  ".NWLSKWGKDWDKGGGGGGKDWDKGWKSLWN.",
  ".NWLSKWGKDDDKGGGGGGKDDDKGWKSLWN.",
  ".NWLSKWGKKKKKGGGGGGKKKKKGWKSLWN.",
  ".NWLSKWGGGGGGGGGGGGGGGGGGWKSLWN.",
  ".NWLSKWGGGGGGGGGGGGGGGGGGWKSLWN.",
  ".NWLSKKKKKKKKKKKKKKKKKKKKKKSLWN.",
  ".NWLSSYYYYYYYYYYYYYYYYYYYYSSLWN.",
  ".NWLSSSYYYYYYYYYYYYYYYYYYSSSLWN.",
  ".NWLSSSSSSSSSSSSSSSSSSSSSSSSLWN.",
  ".NWLSSSSSSSSSSSSSSSSSSSSSSSSLWN.",
  ".NWLSSSSSSSSSSSSSSSRRRSSSSSSLWN.",
  ".NWLSSSSSSSSSSSSSSRRRRRSSSSSLWN.",
  ".NWLSSSSSSSSSSSSSSRRRRRSSSSSLWN.",
  ".NWLSSSSSSSSSSSSSSSRRRSSSSSSLWN.",
  ".NWLSSSSSSSSSSSSSSSSSSSSSSSSLWN.",
  ".NWLSSSSSSSSSSSSSSSSSSSSSSSSLWN.",
  ".NWLLLLLLLLLLLLLLLLLLLLLLLLLLWN.",
  ".NWWWWWWWWWWWWWWWWWWWWWWWWWWWWN.",
  "..NNNNNNNNNNNNNNNNNNNNNNNNNNNN..",
  "................................",
  "................................",
  "................................",
];

// Sanity check
if (GRID.length !== H || GRID.some((r) => r.length !== W)) {
  throw new Error(
    `Grid must be ${H} rows of ${W} cols; got ${GRID.length} rows, widths=${GRID
      .map((r) => r.length)
      .join(",")}`
  );
}

// --- build the scaled RGBA buffer -----------------------------------------
const outW = W * SCALE;
const outH = H * SCALE;
const stride = outW * 4;
const raw = Buffer.alloc(outH * (stride + 1)); // +1 byte/row filter

for (let y = 0; y < outH; y++) {
  const rowOffset = y * (stride + 1);
  raw[rowOffset] = 0; // PNG filter: None
  const sy = (y / SCALE) | 0;
  const row = GRID[sy];
  for (let x = 0; x < outW; x++) {
    const sx = (x / SCALE) | 0;
    const ch = row[sx];
    const px = C[ch] || C.T;
    const off = rowOffset + 1 + x * 4;
    raw[off] = px[0];
    raw[off + 1] = px[1];
    raw[off + 2] = px[2];
    raw[off + 3] = px[3];
  }
}

// --- PNG framing ----------------------------------------------------------
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, "ascii");
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(zlib.crc32(Buffer.concat([typeBuf, data])) >>> 0, 0);
  return Buffer.concat([len, typeBuf, data, crc]);
}

const SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(outW, 0);
ihdr.writeUInt32BE(outH, 4);
ihdr.writeUInt8(8, 8);   // bit depth
ihdr.writeUInt8(6, 9);   // color type 6 = RGBA
ihdr.writeUInt8(0, 10);  // compression
ihdr.writeUInt8(0, 11);  // filter
ihdr.writeUInt8(0, 12);  // interlace

const idat = zlib.deflateSync(raw, { level: 9 });

const png = Buffer.concat([
  SIGNATURE,
  chunk("IHDR", ihdr),
  chunk("IDAT", idat),
  chunk("IEND", Buffer.alloc(0)),
]);

fs.mkdirSync(path.dirname(OUT), { recursive: true });
fs.writeFileSync(OUT, png);
console.log(`Wrote ${png.length} bytes → ${OUT} (${outW}×${outH})`);
