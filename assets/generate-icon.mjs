import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const PNG_SIZE = 256;
const ICO_SIZES = [16, 24, 32, 48, 64, 256];
const GRADIENT_STEPS = 1024;
const GRADIENT_STOPS = [
  [0.0, "#6D28D9"],
  [0.36, "#0EA5E9"],
  [0.56, "#06B6D4"],
  [1.0, "#EC4899"],
];
const GLOW_COLOR = "#FFFFFF";
const DEPTH_COLOR = "#1E1B4B";
const SHADOW_COLOR = "#1E1B4B";
const GLYPH_TOP = "#FFFFFF";
const GLYPH_BOTTOM = "#E8EEFF";
const ARROW_OUTLINE = [
  [0.0, 0.0],
  [0.0, 0.79],
  [0.2, 0.615],
  [0.335, 0.915],
  [0.47, 0.855],
  [0.335, 0.56],
  [0.575, 0.56],
];

const clamp = (value, low, high) => Math.min(high, Math.max(low, value));
const mix = (from, to, amount) => from + (to - from) * amount;
const mixColor = (from, to, amount) => from.map((channel, index) => mix(channel, to[index], amount));

function smoothstep(edge0, edge1, value) {
  const t = clamp((value - edge0) / (edge1 - edge0), 0, 1);
  return t * t * (3 - 2 * t);
}

function hexToSrgb(hex) {
  const packed = Number.parseInt(hex.slice(1), 16);
  return [(packed >> 16) & 255, (packed >> 8) & 255, packed & 255].map((channel) => channel / 255);
}

function srgbToLinear(channel) {
  return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
}

function linearToSrgb(channel) {
  const value = clamp(channel, 0, 1);
  return value <= 0.0031308 ? value * 12.92 : 1.055 * value ** (1 / 2.4) - 0.055;
}

function linearColor(hex) {
  return hexToSrgb(hex).map(srgbToLinear);
}

function linearToOklab([red, green, blue]) {
  const l = Math.cbrt(0.4122214708 * red + 0.5363325363 * green + 0.0514459929 * blue);
  const m = Math.cbrt(0.2119034982 * red + 0.6806995451 * green + 0.1073969566 * blue);
  const s = Math.cbrt(0.0883024619 * red + 0.2817188376 * green + 0.6299787005 * blue);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}

function oklabToLinear([lightness, a, b]) {
  const l = (lightness + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (lightness - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (lightness - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ].map((channel) => clamp(channel, 0, 1));
}

function toOklch(hex) {
  const [lightness, a, b] = linearToOklab(linearColor(hex));
  return [lightness, Math.hypot(a, b), Math.atan2(b, a)];
}

function hueBetween(from, to, amount) {
  let delta = to - from;
  while (delta > Math.PI) delta -= 2 * Math.PI;
  while (delta < -Math.PI) delta += 2 * Math.PI;
  return from + delta * amount;
}

function oklchBetween(from, to, amount) {
  const lightness = mix(from[0], to[0], amount);
  const chroma = mix(from[1], to[1], amount);
  const hue = hueBetween(from[2], to[2], amount);
  return oklabToLinear([lightness, chroma * Math.cos(hue), chroma * Math.sin(hue)]);
}

function gradientColor(stops, position) {
  const upper = Math.max(1, stops.findIndex(([offset]) => offset >= position));
  const [startOffset, startColor] = stops[upper - 1];
  const [endOffset, endColor] = stops[upper];
  const amount = smoothstep(0, 1, clamp((position - startOffset) / (endOffset - startOffset), 0, 1));
  return oklchBetween(startColor, endColor, amount);
}

function gradientTable() {
  const stops = GRADIENT_STOPS.map(([offset, hex]) => [offset, toOklch(hex)]);
  return Array.from({ length: GRADIENT_STEPS }, (_, index) => gradientColor(stops, index / (GRADIENT_STEPS - 1)));
}

const GRADIENT = gradientTable();
const GLOW = linearColor(GLOW_COLOR);
const DEPTH = linearColor(DEPTH_COLOR);
const SHADOW = linearColor(SHADOW_COLOR);
const GLYPH_LIGHT = linearColor(GLYPH_TOP);
const GLYPH_SHADE = linearColor(GLYPH_BOTTOM);

function sdSuperellipse(x, y, half, exponent) {
  const u = Math.abs(x) / half;
  const v = Math.abs(y) / half;
  const norm = (u ** exponent + v ** exponent) ** (1 / exponent);
  const slope = Math.hypot(u ** (exponent - 1), v ** (exponent - 1)) * norm ** (1 - exponent);
  if (slope < 1e-6) return -half;
  return ((norm - 1) * half) / slope;
}

function sdCircle(x, y, radius) {
  return Math.hypot(x, y) - radius;
}

function sdUnevenCapsule(across, along, startRadius, endRadius, height) {
  const x = Math.abs(across);
  const b = (startRadius - endRadius) / height;
  const a = Math.sqrt(1 - b * b);
  const k = -b * x + a * along;
  if (k < 0) return Math.hypot(x, along) - startRadius;
  if (k > a * height) return Math.hypot(x, along - height) - endRadius;
  return a * x + b * along - startRadius;
}

function sdPolygon(x, y, points) {
  let distance = Infinity;
  let sign = 1;
  for (let index = 0, previous = points.length - 1; index < points.length; previous = index, index += 1) {
    const [ax, ay] = points[previous];
    const [bx, by] = points[index];
    const ex = bx - ax;
    const ey = by - ay;
    const wx = x - ax;
    const wy = y - ay;
    const h = clamp((wx * ex + wy * ey) / (ex * ex + ey * ey), 0, 1);
    distance = Math.min(distance, Math.hypot(wx - ex * h, wy - ey * h));
    const crosses = (y >= ay) !== (y >= by) && x < ax + ((y - ay) * ex) / ey;
    if (crosses) sign = -sign;
  }
  return sign * distance;
}

function nearestSpoke(x, y, count) {
  const sector = (2 * Math.PI) / count;
  const angle = Math.atan2(y, x) + sector / 2;
  const local = angle - Math.round(angle / sector) * sector;
  const radius = Math.hypot(x, y);
  return [radius * Math.cos(local), radius * Math.sin(local)];
}

function sdSpoke(x, y, helm) {
  const [along, across] = nearestSpoke(x, y, helm.spokes);
  const inner = sdUnevenCapsule(across, along, helm.spokeWidth, helm.spokeWidth, helm.ring);
  const handleLength = helm.reach - helm.ring;
  const handle = sdUnevenCapsule(across, along - helm.ring, helm.handleBase, helm.knob, handleLength);
  return Math.min(helm.innerSpokes ? inner : Infinity, handle);
}

function sdHelm(x, y, helm) {
  const ring = Math.abs(Math.hypot(x, y) - helm.ring) - helm.ringWidth;
  const hub = sdCircle(x, y, helm.hub);
  const hubWithHole = helm.hole > 0 ? Math.max(hub, -sdCircle(x, y, helm.hole)) : hub;
  return Math.min(ring, hubWithHole, sdSpoke(x, y, helm));
}

function sdArrow(x, y, arrow) {
  const scale = arrow.height;
  const localX = (x - arrow.tipX) / scale;
  const localY = (y - arrow.tipY) / scale;
  return sdPolygon(localX, localY, ARROW_OUTLINE) * scale - arrow.rounding;
}

function sdGlyph(x, y, spec) {
  const arrow = sdArrow(x, y, spec.arrow);
  const helm = sdHelm(x - spec.helm.centerX, y - spec.helm.centerY, spec.helm);
  const cutHelm = Math.max(helm, -(arrow - spec.gap));
  return Math.min(cutHelm, arrow);
}

function coverage(distance, sampleWidth) {
  return clamp(0.5 - distance / sampleWidth, 0, 1);
}

function background(x, y, spec) {
  const position = clamp((x + y) * 0.56 - 0.06, 0, 1);
  let color = GRADIENT[Math.round(position * (GRADIENT_STEPS - 1))];
  const glow = 1 - smoothstep(0, 0.6, Math.hypot(x - 0.12, y - 0.06));
  color = mixColor(color, GLOW, 0.1 * spec.lighting * glow * glow);
  return mixColor(color, DEPTH, 0.14 * smoothstep(0.45, 1.1, y));
}

function sheen(x, y, plateDistance, spec) {
  const top = 0.1 * spec.lighting * (1 - smoothstep(0, 0.6, y));
  const rim = spec.rim * smoothstep(-spec.rimWidth, 0, plateDistance) * (1 - smoothstep(0.05, 0.7, y));
  return clamp(top + rim, 0, 1);
}

function shadowAlpha(x, y, spec) {
  const distance = sdGlyph(x - spec.shadowOffsetX, y - spec.shadowOffsetY, spec);
  return spec.shadowStrength * (1 - smoothstep(-spec.shadowSoftness * 0.2, spec.shadowSoftness, distance));
}

function glyphColor(y) {
  return mixColor(GLYPH_LIGHT, GLYPH_SHADE, smoothstep(0.15, 0.9, y));
}

function shade(x, y, spec) {
  const plateDistance = sdSuperellipse(x - 0.5, y - 0.5, 0.5, spec.exponent);
  const plate = coverage(plateDistance, spec.sampleWidth);
  if (plate <= 0) return [0, 0, 0, 0];
  let color = background(x, y, spec);
  color = mixColor(color, GLOW, sheen(x, y, plateDistance, spec));
  color = mixColor(color, SHADOW, shadowAlpha(x, y, spec));
  color = mixColor(color, glyphColor(y), coverage(sdGlyph(x, y, spec), spec.sampleWidth));
  return [color[0] * plate, color[1] * plate, color[2] * plate, plate];
}

const LAYOUTS = {
  16: {
    lighting: 0,
    margin: 1,
    helm: { center: 6.5, ring: 3, ringWidth: 0.6, hub: 0.8, hole: 0, inner: 0, base: 0.5, knob: 0.72, reach: 5.2 },
    arrow: { tip: 8.3, height: 6.6, rounding: 0.3 },
    gap: 0.95,
    shadow: { offset: 0.5, softness: 1.2, strength: 0.2 },
  },
  24: {
    lighting: 0.25,
    margin: 1,
    helm: { center: 10.5, ring: 4, ringWidth: 0.6, hub: 1.3, hole: 0, inner: 0, base: 0.55, knob: 0.85, reach: 7.2 },
    arrow: { tip: 12.4, height: 10.2, rounding: 0.4 },
    gap: 1.1,
    shadow: { offset: 0.6, softness: 1.4, strength: 0.24 },
  },
  32: {
    lighting: 0.5,
    margin: 1,
    helm: { center: 13.5, ring: 6, ringWidth: 0.8, hub: 1.8, hole: 0, inner: 0, base: 0.75, knob: 1.15, reach: 9.8 },
    arrow: { tip: 16, height: 13.4, rounding: 0.5 },
    gap: 1.3,
    shadow: { offset: 0.7, softness: 1.8, strength: 0.28 },
  },
  48: {
    lighting: 0.85,
    margin: 2,
    helm: { center: 20.5, ring: 9, ringWidth: 1.1, hub: 3.2, hole: 1.2, inner: 0.7, base: 1, knob: 1.7, reach: 14.3 },
    arrow: { tip: 22.6, height: 20.4, rounding: 0.8 },
    gap: 1.6,
    shadow: { offset: 1, softness: 2.6, strength: 0.32 },
  },
  64: {
    lighting: 1,
    margin: 3,
    helm: { center: 27.5, ring: 12, ringWidth: 1.4, hub: 4.3, hole: 1.6, inner: 0.85, base: 1.2, knob: 2.3, reach: 19 },
    arrow: { tip: 30, height: 27.2, rounding: 1 },
    gap: 2,
    shadow: { offset: 1.3, softness: 3.4, strength: 0.34 },
  },
  256: {
    lighting: 1,
    margin: 10,
    helm: { center: 108, ring: 47, ringWidth: 7, hub: 17.5, hole: 7.5, inner: 4.6, base: 5.2, knob: 9, reach: 72 },
    arrow: { tip: 118, height: 104, rounding: 3.4 },
    gap: 7,
    shadow: { offset: 5, softness: 14, strength: 0.38 },
  },
};

function plateMapping(size, marginPixels) {
  const plateScale = 1 - (2 * marginPixels) / size;
  const length = (pixels) => pixels / (size * plateScale);
  const position = (pixels) => (pixels / size - marginPixels / size) / plateScale;
  return { margin: marginPixels / size, plateScale, length, position };
}

function helmSpec(helm, map) {
  return {
    centerX: map.position(helm.center),
    centerY: map.position(helm.center),
    spokes: 8,
    innerSpokes: helm.inner > 0,
    ring: map.length(helm.ring),
    ringWidth: map.length(helm.ringWidth),
    hub: map.length(helm.hub),
    hole: map.length(helm.hole),
    spokeWidth: map.length(helm.inner),
    handleBase: map.length(helm.base),
    knob: map.length(helm.knob),
    reach: map.length(helm.reach),
  };
}

function arrowSpec(arrow, map) {
  return {
    tipX: map.position(arrow.tip),
    tipY: map.position(arrow.tip),
    height: map.length(arrow.height),
    rounding: map.length(arrow.rounding),
  };
}

function specFor(size) {
  const layout = LAYOUTS[size];
  const map = plateMapping(size, layout.margin);
  const samples = size >= 128 ? 6 : 12;
  return {
    size,
    samples,
    margin: map.margin,
    plateScale: map.plateScale,
    sampleWidth: map.length(1) / samples,
    exponent: 5,
    lighting: layout.lighting,
    rim: 0.35 * layout.lighting,
    rimWidth: Math.max(0.02, map.length(1)),
    gap: map.length(layout.gap),
    shadowOffsetX: map.length(layout.shadow.offset) * 0.3,
    shadowOffsetY: map.length(layout.shadow.offset),
    shadowSoftness: map.length(layout.shadow.softness),
    shadowStrength: layout.shadow.strength,
    helm: helmSpec(layout.helm, map),
    arrow: arrowSpec(layout.arrow, map),
  };
}

function samplePixel(spec, column, row) {
  const total = [0, 0, 0, 0];
  for (let sy = 0; sy < spec.samples; sy += 1) {
    for (let sx = 0; sx < spec.samples; sx += 1) {
      const u = (column + (sx + 0.5) / spec.samples) / spec.size;
      const v = (row + (sy + 0.5) / spec.samples) / spec.size;
      const color = shade((u - spec.margin) / spec.plateScale, (v - spec.margin) / spec.plateScale, spec);
      for (let channel = 0; channel < 4; channel += 1) total[channel] += color[channel];
    }
  }
  return total.map((channel) => channel / (spec.samples * spec.samples));
}

function toStraightRgba([red, green, blue, alpha]) {
  if (alpha <= 0) return [0, 0, 0, 0];
  const straight = [red, green, blue].map((channel) => Math.round(linearToSrgb(channel / alpha) * 255));
  return [...straight, Math.round(alpha * 255)];
}

export function renderIcon(size) {
  const spec = specFor(size);
  const pixels = new Uint8Array(size * size * 4);
  for (let row = 0; row < size; row += 1) {
    for (let column = 0; column < size; column += 1) {
      pixels.set(toStraightRgba(samplePixel(spec, column, row)), (row * size + column) * 4);
    }
  }
  return pixels;
}

function crcTable() {
  return Array.from({ length: 256 }, (_, index) => {
    let crc = index;
    for (let bit = 0; bit < 8; bit += 1) crc = crc & 1 ? 0xedb88320 ^ (crc >>> 1) : crc >>> 1;
    return crc >>> 0;
  });
}

const CRC_TABLE = crcTable();

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) crc = CRC_TABLE[(crc ^ byte) & 255] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const typed = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(typed));
  return Buffer.concat([length, typed, crc]);
}

function paeth(left, up, upLeft) {
  const estimate = left + up - upLeft;
  const toLeft = Math.abs(estimate - left);
  const toUp = Math.abs(estimate - up);
  const toUpLeft = Math.abs(estimate - upLeft);
  if (toLeft <= toUp && toLeft <= toUpLeft) return left;
  return toUp <= toUpLeft ? up : upLeft;
}

function filterRow(type, row, previous) {
  return row.map((value, index) => {
    const left = index >= 4 ? row[index - 4] : 0;
    const up = previous[index];
    const upLeft = index >= 4 ? previous[index - 4] : 0;
    const predictors = [0, left, up, (left + up) >> 1, paeth(left, up, upLeft)];
    return (value - predictors[type]) & 255;
  });
}

function bestFilteredRow(row, previous) {
  const candidates = [0, 1, 2, 3, 4].map((type) => [type, filterRow(type, row, previous)]);
  const cost = (bytes) => bytes.reduce((sum, value) => sum + (value < 128 ? value : 256 - value), 0);
  const [type, filtered] = candidates.reduce((best, next) => (cost(next[1]) < cost(best[1]) ? next : best));
  return Buffer.concat([Buffer.from([type]), Buffer.from(filtered)]);
}

function filteredScanlines(pixels, size) {
  const stride = size * 4;
  const rows = [];
  let previous = new Uint8Array(stride);
  for (let row = 0; row < size; row += 1) {
    const current = pixels.subarray(row * stride, (row + 1) * stride);
    rows.push(bestFilteredRow(current, previous));
    previous = current;
  }
  return Buffer.concat(rows);
}

export function encodePng(pixels, size) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(size, 0);
  header.writeUInt32BE(size, 4);
  header.set([8, 6, 0, 0, 0], 8);
  const signature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const data = deflateSync(filteredScanlines(pixels, size), { level: 9 });
  return Buffer.concat([signature, pngChunk("IHDR", header), pngChunk("IDAT", data), pngChunk("IEND", Buffer.alloc(0))]);
}

function icoEntry(size, length, offset) {
  const entry = Buffer.alloc(16);
  entry.writeUInt8(size >= 256 ? 0 : size, 0);
  entry.writeUInt8(size >= 256 ? 0 : size, 1);
  entry.writeUInt16LE(1, 4);
  entry.writeUInt16LE(32, 6);
  entry.writeUInt32LE(length, 8);
  entry.writeUInt32LE(offset, 12);
  return entry;
}

export function encodeIco(images) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(images.length, 4);
  let offset = header.length + images.length * 16;
  const entries = images.map(({ size, png }) => {
    const entry = icoEntry(size, png.length, offset);
    offset += png.length;
    return entry;
  });
  return Buffer.concat([header, ...entries, ...images.map(({ png }) => png)]);
}

function writeAssets(folder) {
  mkdirSync(folder, { recursive: true });
  const images = ICO_SIZES.map((size) => ({ size, png: encodePng(renderIcon(size), size) }));
  const large = images.find(({ size }) => size === PNG_SIZE);
  writeFileSync(join(folder, "deckhand.png"), large.png);
  writeFileSync(join(folder, "deckhand.ico"), encodeIco(images));
}

const invokedDirectly = process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  writeAssets(dirname(fileURLToPath(import.meta.url)));
}
