import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { crc32, deflateSync } from "node:zlib";

const REPO_URL = "github.com/your-handle/deckhand";

const DEMO = dirname(fileURLToPath(import.meta.url));
const WORK = join(DEMO, "work");
const CARDS = join(DEMO, "cards");
const ICON = resolve(DEMO, "..", "assets", "deckhand.png");
const FONT_DIR = "C:/Windows/Fonts";
const FONTS = {
  bold: "segoeuib.ttf",
  semibold: "seguisb.ttf",
  regular: "segoeui.ttf",
  semilight: "segoeuisl.ttf",
  mono: "CascadiaMono.ttf",
};

const W = 1920;
const H = 1080;
const FPS = 30;
const INTRO_SECONDS = 3.0;
const OUTRO_SECONDS = 3.6;
const CROSSFADE = 0.5;
const MAX_SECONDS = 29.5;
const FOOTAGE_BOX = { width: 1640, height: 1000 };
const BACKDROP = "#0B0D1A";
const AURORA = ["#7C3AED", "#4F46E5", "#0EA5E9", "#22D3EE", "#F472B6"];
const GIF_BUDGET = 8 * 1024 * 1024;
const GIF_TRIES = [
  { width: 960, fps: 15, dither: "sierra2_4a" },
  { width: 960, fps: 15, dither: "bayer:bayer_scale=4" },
  { width: 960, fps: 12, dither: "bayer:bayer_scale=4" },
  { width: 840, fps: 12, dither: "bayer:bayer_scale=3" },
  { width: 800, fps: 12, dither: "bayer:bayer_scale=3" },
  { width: 720, fps: 12, dither: "bayer:bayer_scale=3" },
  { width: 720, fps: 10, dither: "bayer:bayer_scale=3" },
];

const args = parseArgs(process.argv.slice(2));

function parseArgs(list) {
  const parsed = {};
  for (let index = 0; index < list.length; index++) {
    const key = list[index].replace(/^--/, "");
    const next = list[index + 1];
    if (next === undefined || next.startsWith("--")) {
      parsed[key] = true;
    } else {
      parsed[key] = next;
      index++;
    }
  }
  return parsed;
}

function font(weight) {
  const path = `${FONT_DIR}/${FONTS[weight]}`;
  return existsSync(path) ? path : `${FONT_DIR}/segoeui.ttf`;
}

function ffmpeg(list, { info = false } = {}) {
  const result = spawnSync("ffmpeg", ["-hide_banner", "-y", "-loglevel", info ? "info" : "error", ...list], {
    encoding: "utf8",
    maxBuffer: 256 * 1024 * 1024,
  });
  if (result.status !== 0) throw new Error(`ffmpeg failed: ${list.join(" ")}\n${result.stderr}`);
  return result.stderr;
}

function probe(path) {
  const result = spawnSync(
    "ffprobe",
    ["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height:format=duration", "-of", "json", path],
    { encoding: "utf8" },
  );
  if (result.status !== 0) throw new Error(`ffprobe failed on ${path}\n${result.stderr}`);
  const json = JSON.parse(result.stdout);
  return { width: json.streams[0].width, height: json.streams[0].height, duration: Number(json.format.duration) };
}

const quoted = (path) => `'${path.replace(/\\/g, "/").replace(/:/g, "\\:")}'`;
const work = (name) => join(WORK, name);
const even = (value) => Math.round(value / 2) * 2;
const clamp = (value, low, high) => Math.min(high, Math.max(low, value));
const smoothstep = (edge0, edge1, value) => {
  const t = clamp((value - edge0) / (edge1 - edge0), 0, 1);
  return t * t * (3 - 2 * t);
};
const fixed = (value) => Number(value.toFixed(3));

function graph(name, text) {
  const path = work(`${name}.graph`);
  writeFileSync(path, text);
  return path;
}

function textFile(text) {
  const path = work(`text-${createHash("sha1").update(text).digest("hex").slice(0, 10)}.txt`);
  writeFileSync(path, text);
  return path;
}

function drawtext({ text, weight, size, color, x, y }) {
  return `drawtext=fontfile=${quoted(font(weight))}:textfile=${quoted(textFile(text))}:expansion=none:fontsize=${size}:fontcolor=${color}:x=${x}:y=${y}:y_align=baseline`;
}

function measure(text, weight, size) {
  const draw = drawtext({ text, weight, size, color: "white", x: 200, y: 300 });
  const log = ffmpeg(["-f", "lavfi", "-i", "color=black:s=4000x500", "-vf", `${draw},bbox=min_val=20`, "-frames:v", "1", "-f", "null", "-"], {
    info: true,
  });
  const found = log.match(/x1:(\d+) x2:(\d+) y1:(\d+) y2:(\d+)/);
  if (!found) throw new Error(`could not measure "${text}"`);
  const [x1, x2, y1, y2] = found.slice(1).map(Number);
  return { left: x1 - 200, width: x2 - x1 + 1, ascent: 300 - y1, descent: y2 - 300 };
}

const hex = (value) => [1, 3, 5].map((at) => Number.parseInt(value.slice(at, at + 2), 16) / 255);
const toLinear = (channel) => (channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4);
const toSrgb = (channel) => {
  const value = clamp(channel, 0, 1);
  return value <= 0.0031308 ? value * 12.92 : 1.055 * value ** (1 / 2.4) - 0.055;
};
const linear = (color) => color.map(toLinear);
const srgb = (color) => color.map(toSrgb);
const AURORA_LINEAR = AURORA.map((color) => linear(hex(color)));

function mixLinear(from, to, amount) {
  return from.map((channel, index) => channel + (to[index] - channel) * amount);
}

function auroraAt(position, loop = false) {
  const stops = loop ? [...AURORA_LINEAR, AURORA_LINEAR[0]] : AURORA_LINEAR;
  const scaled = clamp(loop ? ((position % 1) + 1) % 1 : position, 0, 1) * (stops.length - 1);
  const index = Math.min(stops.length - 2, Math.floor(scaled));
  return srgb(mixLinear(stops[index], stops[index + 1], smoothstep(0, 1, scaled - index)));
}

function over(below, above) {
  const alpha = above[3] + below[3] * (1 - above[3]);
  if (alpha <= 0) return [0, 0, 0, 0];
  const mix = (index) => (above[index] * above[3] + below[index] * below[3] * (1 - above[3])) / alpha;
  return [mix(0), mix(1), mix(2), alpha];
}

function roundedBox(x, y, box) {
  const qx = Math.abs(x - box.cx) - (box.hw - box.r);
  const qy = Math.abs(y - box.cy) - (box.hh - box.r);
  return Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - box.r;
}

const coverage = (distance) => clamp(0.5 - distance, 0, 1);

function noise(x, y, seed) {
  let hash = (Math.imul(x, 374761393) + Math.imul(y, 668265263) + Math.imul(seed, 1442695041)) | 0;
  hash = Math.imul(hash ^ (hash >>> 13), 1274126177);
  hash ^= hash >>> 16;
  return (hash >>> 0) / 4294967296;
}

function pngChunk(type, data) {
  const head = Buffer.alloc(8);
  head.writeUInt32BE(data.length, 0);
  head.write(type, 4, "ascii");
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), data])) >>> 0, 0);
  return Buffer.concat([head, data, crc]);
}

function paint(path, width, height, shade) {
  const stride = width * 4 + 1;
  const raw = Buffer.alloc(stride * height);
  for (let y = 0; y < height; y++) {
    raw[y * stride] = 0;
    for (let x = 0; x < width; x++) {
      const [red, green, blue, alpha] = shade(x + 0.5, y + 0.5);
      const grain = (noise(x, y, 1) + noise(x, y, 2) - 1) * 0.7;
      const at = y * stride + 1 + x * 4;
      raw[at] = clamp(Math.round(red * 255 + grain), 0, 255);
      raw[at + 1] = clamp(Math.round(green * 255 + grain), 0, 255);
      raw[at + 2] = clamp(Math.round(blue * 255 + grain), 0, 255);
      raw[at + 3] = clamp(Math.round(alpha * 255), 0, 255);
    }
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header.set([8, 6, 0, 0, 0], 8);
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const chunks = [pngChunk("IHDR", header), pngChunk("IDAT", deflateSync(raw, { level: 6 })), pngChunk("IEND", Buffer.alloc(0))];
  writeFileSync(path, Buffer.concat([signature, ...chunks]));
  return path;
}

function backdropShade({ blobs, vignette }) {
  const base = linear(hex(BACKDROP));
  const lights = blobs.map((blob) => ({ ...blob, light: linear(hex(blob.color)) }));
  return (x, y) => {
    const color = [...base];
    for (const blob of lights) {
      const weight = blob.alpha * Math.exp(-((x - blob.x * W) ** 2 + (y - blob.y * H) ** 2) / (2 * blob.sigma ** 2));
      for (let channel = 0; channel < 3; channel++) color[channel] += blob.light[channel] * weight;
    }
    const reach = Math.hypot((x - W / 2) / (W / 2), (y - H / 2) / (H / 2));
    const dim = 1 - vignette * smoothstep(0.6, 1.3, reach);
    return [...srgb(color.map((channel) => channel * dim)), 1];
  };
}

function renderCanvas() {
  return paint(work("canvas.png"), W, H, backdropShade({
    vignette: 0.3,
    blobs: [
      { x: -0.04, y: 0.1, sigma: 520, color: "#7C3AED", alpha: 0.2 },
      { x: 0.3, y: -0.28, sigma: 480, color: "#4F46E5", alpha: 0.12 },
      { x: 1.05, y: 0.04, sigma: 440, color: "#0EA5E9", alpha: 0.09 },
      { x: 1.04, y: 0.96, sigma: 520, color: "#22D3EE", alpha: 0.1 },
      { x: 0.78, y: 1.2, sigma: 420, color: "#F472B6", alpha: 0.1 },
      { x: 0.06, y: 1.12, sigma: 380, color: "#F472B6", alpha: 0.05 },
    ],
  }));
}

function renderCardCanvas(name) {
  return paint(work(`${name}-canvas.png`), W, H, backdropShade({
    vignette: 0.4,
    blobs: [
      { x: 0.12, y: -0.12, sigma: 520, color: "#7C3AED", alpha: 0.12 },
      { x: 0.92, y: 1.12, sigma: 560, color: "#22D3EE", alpha: 0.07 },
      { x: 0.96, y: -0.08, sigma: 380, color: "#F472B6", alpha: 0.05 },
      { x: 0.04, y: 1.08, sigma: 420, color: "#0EA5E9", alpha: 0.05 },
    ],
  }));
}

function renderHalo(name, size, ring) {
  const center = size / 2;
  return paint(work(`${name}-halo.png`), size, size, (x, y) => {
    const dx = x - center;
    const dy = y - center;
    const radius = Math.hypot(dx, dy);
    const color = auroraAt(Math.atan2(dy, dx) / (2 * Math.PI) + 0.5, true);
    const band = ring.alpha * Math.exp(-((radius - ring.radius) ** 2) / (2 * ring.width ** 2));
    const glow = ring.glow * Math.exp(-(radius ** 2) / (2 * ring.spread ** 2));
    return [...color, clamp(band + glow, 0, 1)];
  });
}

function renderMask(width, height, feather) {
  const box = { cx: width / 2, cy: height / 2, hw: width / 2, hh: height / 2, r: feather * 1.4 };
  return paint(work("mask.png"), width, height, (x, y) => {
    const inside = smoothstep(0, 1, clamp(-roundedBox(x, y, box) / feather, 0, 1));
    return [inside, inside, inside, 1];
  });
}

function renderGradient(name, width, height, from, to) {
  return paint(work(`${name}.png`), width, height, (x) => [...auroraAt((x - from) / (to - from)), 1]);
}

function renderPillBox(name, inner, style) {
  const pad = style.pad;
  const box = { cx: pad + inner.width / 2, cy: pad + inner.height / 2, hw: inner.width / 2, hh: inner.height / 2, r: inner.height / 2 };
  const shadowBox = { ...box, cy: box.cy + style.shadowDrop };
  const fill = [...hex(style.fill), style.fillAlpha];
  return paint(work(`${name}.png`), inner.width + pad * 2, inner.height + pad * 2, (x, y) => {
    const shadow = style.shadowAlpha * (1 - smoothstep(-style.shadowBlur, style.shadowBlur * 1.2, roundedBox(x, y, shadowBox)));
    let color = [0, 0, 0, shadow];
    const distance = roundedBox(x, y, box);
    color = over(color, [fill[0], fill[1], fill[2], fill[3] * coverage(distance)]);
    const ring = coverage(distance) - coverage(distance + style.border);
    color = over(color, [...auroraAt((x - pad) / inner.width), ring * style.borderAlpha]);
    for (const extra of style.extras ?? []) color = over(color, extra(x, y));
    return color;
  });
}

function chipShape(left, top, width, height) {
  const box = { cx: left + width / 2, cy: top + height / 2, hw: width / 2, hh: height / 2, r: 11 };
  const tint = hex("#22D3EE");
  return (x, y) => {
    const distance = roundedBox(x, y, box);
    const fill = coverage(distance) * 0.13;
    const ring = (coverage(distance) - coverage(distance + 1.2)) * 0.45;
    return [...tint, clamp(fill + ring, 0, 1)];
  };
}

function dotShape(cx, cy, radius) {
  return (x, y) => {
    const distance = Math.hypot(x - cx, y - cy);
    const core = coverage(distance - radius);
    const glow = 0.45 * Math.exp(-(distance ** 2) / (2 * (radius * 2.2) ** 2));
    const color = auroraAt(clamp((y - (cy - radius)) / (radius * 2), 0, 1) * 0.6 + 0.2);
    return [...(core > 0 ? mixLinear(color, [1, 1, 1], 0.15 * core) : color), clamp(core + glow * (1 - core), 0, 1)];
  };
}

const CAPTION = {
  height: 78,
  size: 36,
  chipSize: 26,
  pad: 44,
  dotX: 32,
  textX: 56,
  right: 32,
  chipGap: 16,
  chipPad: 14,
  chipHeight: 42,
};

function renderCaption(index, caption) {
  const main = measure(caption.text, "semibold", CAPTION.size);
  const chip = caption.chip ? measure(caption.chip, "mono", CAPTION.chipSize) : null;
  const chipWidth = chip ? chip.width + CAPTION.chipPad * 2 : 0;
  const width = CAPTION.textX + main.width + (chip ? CAPTION.chipGap + chipWidth : 0) + CAPTION.right;
  const pad = CAPTION.pad;
  const centerY = pad + CAPTION.height / 2;
  const chipLeft = pad + CAPTION.textX + main.width + CAPTION.chipGap;
  const extras = [dotShape(pad + CAPTION.dotX, centerY, 6.5)];
  if (chip) extras.push(chipShape(chipLeft, centerY - CAPTION.chipHeight / 2, chipWidth, CAPTION.chipHeight));
  const box = renderPillBox(`caption-${index}-box`, { width, height: CAPTION.height }, {
    pad,
    fill: "#0E1021",
    fillAlpha: 0.88,
    border: 1.6,
    borderAlpha: 0.9,
    shadowAlpha: 0.55,
    shadowBlur: 18,
    shadowDrop: 10,
    extras,
  });
  const baseline = Math.round(centerY + CAPTION.size * 0.35);
  const draws = [drawtext({ text: caption.text, weight: "semibold", size: CAPTION.size, color: "0xF4F5FF", x: pad + CAPTION.textX - main.left, y: baseline })];
  if (chip) {
    const chipBaseline = Math.round(centerY + CAPTION.chipSize * 0.36);
    draws.push(drawtext({ text: caption.chip, weight: "mono", size: CAPTION.chipSize, color: "0x8EF0FB", x: chipLeft + CAPTION.chipPad - chip.left, y: chipBaseline }));
  }
  const path = work(`caption-${index}.png`);
  ffmpeg(["-i", box, "-vf", `format=rgba,${draws.join(",")}`, "-frames:v", "1", path]);
  return { path, width: width + pad * 2, height: CAPTION.height + pad * 2, pad };
}

function textLayer(name, spec) {
  const path = work(`${name}.png`);
  const draw = drawtext({ ...spec, x: spec.x ?? "(w-text_w)/2" });
  ffmpeg(["-f", "lavfi", "-i", `color=black@0:s=${W}x${spec.height},format=rgba`, "-vf", `format=rgba,${draw}`, "-frames:v", "1", path]);
  return path;
}

function gradientTextLayer(name, spec) {
  const plain = textLayer(`${name}-plain`, { ...spec, color: "white" });
  const size = measure(spec.text, spec.weight, spec.size);
  const left = (W - size.width) / 2;
  const fill = renderGradient(`${name}-fill`, W, spec.height, left, left + size.width);
  const path = work(`${name}.png`);
  ffmpeg(["-i", fill, "-i", plain, "-filter_complex", "[1:v]format=rgba,alphaextract[a];[0:v]format=rgba[f];[f][a]alphamerge", "-frames:v", "1", path]);
  return path;
}

const eased = (start, duration) => `pow(1-clip((t-${start})/${duration},0,1),3)`;

const LOSSLESS = ["-pix_fmt", "bgr24", "-c:v", "libx264rgb", "-qp", "0", "-preset", "veryfast"];

function still(path, seconds) {
  return ["-framerate", String(FPS), "-loop", "1", "-t", String(seconds), "-i", path];
}

function renderCard(name, seconds, layers, exit = null) {
  const inputs = [...still(renderCardCanvas(name), seconds)];
  const chains = ["[0:v]format=rgba[base0]"];
  let current = "base0";
  layers.forEach((layer, index) => {
    inputs.push(...still(layer.path, seconds));
    const source = `[${index + 1}:v]`;
    const label = `l${index}`;
    const leave = exit ? `,fade=t=out:st=${exit.start}:d=${exit.fade}:alpha=1` : "";
    chains.push(`${source}format=rgba${layer.filter ? `,${layer.filter}` : ""},fade=t=in:st=${layer.start}:d=${layer.fade}:alpha=1${leave}[${label}]`);
    const next = `base${index + 1}`;
    chains.push(`[${current}][${label}]overlay=x='${layer.x}':y='${layer.y}':eval=frame:format=rgb[${next}]`);
    current = next;
  });
  chains.push(`[${current}]format=gbrp[out]`);
  const lossless = work(`${name}.mkv`);
  ffmpeg([...inputs, "-/filter_complex", graph(name, chains.join(";\n")), "-map", "[out]", "-r", String(FPS), "-t", String(seconds), ...LOSSLESS, lossless]);
  mkdirSync(CARDS, { recursive: true });
  ffmpeg(["-i", lossless, ...deliveryVideo(14), join(CARDS, `${name}.mp4`)]);
  ffmpeg(["-sseof", "-0.1", "-i", lossless, "-frames:v", "1", join(CARDS, `${name}.png`)]);
  return lossless;
}

function iconLayers({ size, centerY, start, from }) {
  const glowSize = Math.round(size * 1.25);
  return [
    {
      path: ICON,
      filter: `scale=${glowSize}:-1,pad=iw+160:ih+160:80:80:color=black@0,gblur=sigma=34,colorchannelmixer=aa=0.6`,
      x: `(W-w)/2`,
      y: `${centerY}-h/2`,
      start: start + 0.05,
      fade: 0.7,
    },
    {
      path: ICON,
      filter: `scale=w='${size}*(${from}+${1 - from}*(1-${eased(start, 0.7)}))':h=-1:eval=frame:flags=lanczos`,
      x: `(W-w)/2`,
      y: `${centerY}-h/2`,
      start,
      fade: 0.45,
    },
  ];
}

function haloLayer(name, centerY, ring, start) {
  const size = Math.round(ring.radius * 2 + ring.spread * 3.2);
  return {
    path: renderHalo(name, even(size), ring),
    filter: `rotate=a='0.18*t':c=none`,
    x: `(W-w)/2`,
    y: `${centerY}-h/2`,
    start,
    fade: 1.1,
  };
}

function riseLayer(path, baselineShift, start, rise = 24, fade = 0.5) {
  return { path, x: 0, y: `${baselineShift}+${rise}*${eased(start, fade + 0.15)}`, start, fade };
}

function renderIntro() {
  const iconY = 392;
  const title = textLayer("intro-title", { text: "Deckhand", weight: "bold", size: 128, color: "white", height: 200, y: 150 });
  const subtitle = textLayer("intro-subtitle", { text: "Let AI agents drive your Windows desktop", weight: "semilight", size: 48, color: "0xC4CAE8", height: 100, y: 66 });
  return renderCard("intro", INTRO_SECONDS, [
    haloLayer("intro", iconY, { radius: 156, width: 24, alpha: 0.5, glow: 0.2, spread: 210 }, 0),
    ...iconLayers({ size: 212, centerY: iconY, start: 0.08, from: 0.84 }),
    riseLayer(title, 700 - 150, 0.36),
    riseLayer(subtitle, 788 - 66, 0.62, 18),
  ], { start: INTRO_SECONDS - CROSSFADE - 0.35, fade: 0.35 });
}

function renderUrlPill() {
  const size = 32;
  const text = measure(REPO_URL, "mono", size);
  const inner = { width: text.width + 76, height: 66 };
  const box = renderPillBox("outro-url-box", inner, {
    pad: 40,
    fill: "#12152B",
    fillAlpha: 0.9,
    border: 1.6,
    borderAlpha: 0.85,
    shadowAlpha: 0.45,
    shadowBlur: 16,
    shadowDrop: 8,
  });
  const path = work("outro-url.png");
  const draw = drawtext({ text: REPO_URL, weight: "mono", size, color: "0xE9ECFF", x: 40 + 38 - text.left, y: Math.round(40 + 33 + size * 0.36) });
  ffmpeg(["-i", box, "-vf", `format=rgba,${draw}`, "-frames:v", "1", path]);
  return { path, height: inner.height + 80 };
}

function renderOutro() {
  const iconY = 290;
  const title = textLayer("outro-title", { text: "Deckhand", weight: "bold", size: 92, color: "white", height: 150, y: 110 });
  const tagline = gradientTextLayer("outro-tagline", { text: "Rust · MCP · open source", weight: "semibold", size: 56, height: 110, y: 75 });
  const speed = textLayer("outro-speed", { text: "Reads a window in ~60 ms", weight: "semilight", size: 40, color: "0xB4BCDC", height: 90, y: 60 });
  const url = renderUrlPill();
  return renderCard("outro", OUTRO_SECONDS, [
    haloLayer("outro", iconY, { radius: 98, width: 18, alpha: 0.45, glow: 0.18, spread: 140 }, 0),
    ...iconLayers({ size: 136, centerY: iconY, start: 0.05, from: 0.88 }),
    riseLayer(title, 486 - 110, 0.15),
    riseLayer(tagline, 588 - 75, 0.32),
    riseLayer(speed, 660 - 60, 0.48, 18),
    { path: url.path, x: "(W-w)/2", y: `${770 - url.height / 2}+18*${eased(0.64, 0.65)}`, start: 0.64, fade: 0.5 },
  ]);
}

function loadTimeline(path) {
  const timeline = JSON.parse(readFileSync(path, "utf8"));
  for (const name of ["read", "fill", "pick", "create", "shot"]) {
    if (!timeline.events?.[name]) throw new Error(`${path} has no "${name}" event`);
  }
  return timeline;
}

function loadEdit() {
  const path = args.edit ? resolve(args.edit) : join(DEMO, "edit.json");
  return existsSync(path) ? JSON.parse(readFileSync(path, "utf8")) : {};
}

function defaultCaptions(events) {
  const chip = events.read.ms && events.read.ms <= 200 ? `${Math.round(events.read.ms)} ms` : null;
  return [
    { text: "Reads the window as text", chip, from: events.read.start - 0.2, to: events.read.start + 1.8 },
    { text: "Clicks and types like a person", from: events.fill.start + 0.1, to: events.fill.start + 2.7 },
    { text: "Every action is visible", from: events.fill.start + 2.95, to: events.fill.end + 0.15 },
    { text: "Handles native file dialogs", from: events.pick.start + 0.1, to: events.pick.end + 0.3 },
    { text: "Waits for real UI events", from: events.create.start + 0.3, to: events.create.end + 0.1 },
    { text: "Screenshots when needed", from: events.shot.start - 0.1, to: events.shot.start + 2.0 },
  ];
}

function spacedCaptions(captions) {
  const sorted = [...captions].sort((a, b) => a.from - b.from);
  return sorted
    .map((caption, index) => {
      const next = sorted[index + 1];
      const to = next ? Math.min(caption.to, next.from - 0.2) : caption.to;
      return { ...caption, to };
    })
    .filter((caption) => caption.to - caption.from >= 0.9);
}

function cuts(events, duration, edit) {
  const head = edit.head ?? Math.max(0, events.read.start - 0.75);
  const tail = Math.min(duration, edit.tail ?? events.shot.end + 1.5);
  return { head, tail };
}

function defaultSpeedups(events) {
  const pick = events.pick;
  return pick.end - pick.start > 2.2 ? [{ from: pick.start + 0.9, to: pick.end - 0.35, speed: 1.6 }] : [];
}

function segmentsBetween(head, tail, speedups) {
  const segments = [];
  let cursor = head;
  for (const speedup of [...speedups].sort((a, b) => a.from - b.from)) {
    const from = clamp(speedup.from, cursor, tail);
    const to = clamp(speedup.to, from, tail);
    if (from > cursor) segments.push({ from: cursor, to: from, speed: 1 });
    if (to > from) segments.push({ from, to, speed: speedup.speed });
    cursor = to;
  }
  if (tail > cursor) segments.push({ from: cursor, to: tail, speed: 1 });
  return segments;
}

function fitted(segments) {
  const footage = segments.reduce((sum, segment) => sum + (segment.to - segment.from) / segment.speed, 0);
  const total = INTRO_SECONDS + footage + OUTRO_SECONDS - 2 * CROSSFADE;
  if (total <= MAX_SECONDS) return segments;
  const factor = footage / (footage - (total - MAX_SECONDS));
  console.log(`footage is ${footage.toFixed(1)} s, speeding it up ${factor.toFixed(2)}x to fit ${MAX_SECONDS} s`);
  return segments.map((segment) => ({ ...segment, speed: segment.speed * factor }));
}

function warpOf(segments) {
  return (time) => {
    let output = 0;
    for (const segment of segments) {
      if (time <= segment.from) break;
      output += (Math.min(time, segment.to) - segment.from) / segment.speed;
    }
    return output;
  };
}

function geometry(timeline, raw) {
  const scale = Math.min(FOOTAGE_BOX.width / raw.width, FOOTAGE_BOX.height / raw.height);
  const width = even(raw.width * scale);
  const height = even(raw.height * scale);
  const left = Math.round((W - width) / 2);
  const top = Math.round((H - height) / 2);
  const region = timeline.region;
  const windowRect = timeline.window;
  const toRaw = region ? raw.width / region.width : 1;
  const margin = region && windowRect ? { x: (windowRect.x - region.x) * toRaw, y: (windowRect.y - region.y) * toRaw } : { x: raw.width * 0.024, y: raw.width * 0.024 };
  const windowSize = windowRect ? { width: windowRect.width * toRaw, height: windowRect.height * toRaw } : { width: raw.width - 2 * margin.x, height: raw.height - 2 * margin.y };
  const feather = Math.max(12, Math.min(margin.x, margin.y) * scale);
  const captionLeft = Math.round(left + (margin.x + 34) * scale);
  const captionBottom = Math.round(top + (margin.y + windowSize.height - 30) * scale);
  return { scale, width, height, left, top, feather, captionLeft, captionBottom };
}

function redactions(edit) {
  return (edit.redact ?? []).map((box) => ({ ...box, sigma: box.sigma ?? 22 }));
}

function footageGraph({ segments, layout, captions, flash, blur, duration }) {
  const lines = [
    `[0:v]setpts=PTS-STARTPTS,fps=${FPS},scale=in_color_matrix=bt709:in_range=auto,format=rgb24[raw0]`,
  ];
  let raw = "raw0";
  blur.forEach((box, index) => {
    lines.push(`[${raw}]split[keep${index}][cut${index}]`);
    lines.push(`[cut${index}]crop=${box.w}:${box.h}:${box.x}:${box.y},gblur=sigma=${box.sigma}[blur${index}]`);
    lines.push(`[keep${index}][blur${index}]overlay=x=${box.x}:y=${box.y}:format=rgb:enable='between(t,${box.from},${box.to})'[raw${index + 1}]`);
    raw = `raw${index + 1}`;
  });
  const parts = segments.map((_, index) => `[part${index}]`).join("");
  lines.push(`[${raw}]split=${segments.length}${parts}`);
  segments.forEach((segment, index) => {
    lines.push(`[part${index}]trim=start=${fixed(segment.from)}:end=${fixed(segment.to)},setpts=(PTS-STARTPTS)/${fixed(segment.speed)}[seg${index}]`);
  });
  const joined = segments.map((_, index) => `[seg${index}]`).join("");
  lines.push(`${joined}concat=n=${segments.length}:v=1:a=0,fps=${FPS},scale=${layout.width}:${layout.height}:flags=lanczos,format=rgba[shot]`);
  lines.push(`[2:v]format=gray,split[mask][flashmask]`);
  lines.push(`[shot][mask]alphamerge[footage]`);
  lines.push(`[1:v]format=rgba[canvas]`);
  lines.push(`[canvas][footage]overlay=x=${layout.left}:y=${layout.top}:format=rgb[comp0]`);
  let comp = "comp0";
  if (flash !== null) {
    lines.push(`color=white:s=${layout.width}x${layout.height}:r=${FPS}:d=${fixed(duration)},format=rgba[white]`);
    lines.push(`[white][flashmask]alphamerge,colorchannelmixer=aa=0.22,fade=t=in:st=${fixed(flash)}:d=0.05:alpha=1,fade=t=out:st=${fixed(flash + 0.07)}:d=0.32:alpha=1[flash]`);
    lines.push(`[${comp}][flash]overlay=x=${layout.left}:y=${layout.top}:format=rgb:enable='between(t,${fixed(flash)},${fixed(flash + 0.5)})'[comp1]`);
    comp = "comp1";
  } else {
    lines.push(`[flashmask]nullsink`);
  }
  captions.forEach((caption, index) => {
    const input = index + 3;
    const fadeOut = fixed(caption.to - 0.2);
    lines.push(`[${input}:v]format=rgba,fade=t=in:st=${fixed(caption.from)}:d=0.24:alpha=1,fade=t=out:st=${fadeOut}:d=0.2:alpha=1[cap${index}]`);
    const x = layout.captionLeft - caption.pad;
    const y = `${layout.captionBottom - caption.height + caption.pad}+16*${eased(fixed(caption.from), 0.38)}`;
    const next = `capcomp${index}`;
    lines.push(`[${comp}][cap${index}]overlay=x=${x}:y='${y}':eval=frame:format=rgb:enable='between(t,${fixed(caption.from)},${fixed(caption.to)})'[${next}]`);
    comp = next;
  });
  lines.push(`[${comp}]format=gbrp[out]`);
  return lines.join(";\n");
}

function renderFootage(rawPath, timeline, edit) {
  const raw = probe(rawPath);
  const events = timeline.events;
  const { head, tail } = cuts(events, raw.duration, edit);
  const segments = fitted(segmentsBetween(head, tail, edit.speeds ?? defaultSpeedups(events)));
  const warp = warpOf(segments);
  const duration = warp(tail);
  const layout = geometry(timeline, raw);
  const captionSpecs = spacedCaptions(edit.captions ?? defaultCaptions(events));
  const captions = captionSpecs.map((caption, index) => ({
    ...renderCaption(index, caption),
    from: warp(caption.from),
    to: Math.min(duration, warp(caption.to)),
  }));
  const flash = edit.flash === false ? null : warp(events.shot.start);
  const inputs = ["-i", rawPath, ...still(renderCanvas(), duration), ...still(renderMask(layout.width, layout.height, layout.feather), duration)];
  for (const caption of captions) inputs.push(...still(caption.path, duration));
  const text = footageGraph({ segments, layout, captions, flash, blur: redactions(edit), duration });
  const lossless = work("footage.mkv");
  ffmpeg([...inputs, "-/filter_complex", graph("footage", text), "-map", "[out]", "-r", String(FPS), "-t", String(fixed(duration)), ...LOSSLESS, lossless]);
  console.log(`footage: ${raw.width}x${raw.height}, cut ${head.toFixed(2)}–${tail.toFixed(2)} s into ${duration.toFixed(2)} s over ${segments.length} segment(s), ${captions.length} captions`);
  return { path: lossless, duration, warp, events, captions };
}

function deliveryVideo(crf) {
  return [
    "-vf",
    "scale=out_color_matrix=bt709:out_range=tv,format=yuv420p",
    "-c:v",
    "libx264",
    "-preset",
    "slow",
    "-crf",
    String(crf),
    "-profile:v",
    "high",
    "-level:v",
    "4.1",
    "-x264-params",
    "aq-mode=3",
    "-pix_fmt",
    "yuv420p",
    "-colorspace",
    "bt709",
    "-color_primaries",
    "bt709",
    "-color_trc",
    "bt709",
    "-color_range",
    "tv",
    "-r",
    String(FPS),
    "-movflags",
    "+faststart",
    "-an",
  ];
}

function assemble(intro, footage, outro) {
  const second = INTRO_SECONDS + footage.duration - 2 * CROSSFADE;
  const text = [
    `[0:v]format=gbrp,settb=1/${FPS}[a]`,
    `[1:v]format=gbrp,settb=1/${FPS}[b]`,
    `[2:v]format=gbrp,settb=1/${FPS}[c]`,
    `[a][b]xfade=transition=fade:duration=${CROSSFADE}:offset=${fixed(INTRO_SECONDS - CROSSFADE)}[ab]`,
    `[ab][c]xfade=transition=fade:duration=${CROSSFADE}:offset=${fixed(second)}[out]`,
  ].join(";\n");
  const master = work("master.mkv");
  ffmpeg(["-i", intro, "-i", footage.path, "-i", outro, "-/filter_complex", graph("assemble", text), "-map", "[out]", ...LOSSLESS, master]);
  return master;
}

function exportMp4(master) {
  const path = join(DEMO, "deckhand-demo.mp4");
  ffmpeg(["-i", master, ...deliveryVideo(18), path]);
  return path;
}

function exportGif(master) {
  const path = join(DEMO, "deckhand-demo.gif");
  for (const attempt of GIF_TRIES) {
    const text = `[0:v]fps=${attempt.fps},scale=${attempt.width}:-2:flags=lanczos,split[a][b];[a]palettegen=max_colors=256:stats_mode=full[p];[b][p]paletteuse=dither=${attempt.dither}:diff_mode=rectangle`;
    ffmpeg(["-i", master, "-/filter_complex", graph("gif", text), "-loop", "0", path]);
    const size = statSync(path).size;
    console.log(`gif ${attempt.width}px ${attempt.fps} fps ${attempt.dither}: ${(size / 1048576).toFixed(2)} MB`);
    if (size <= GIF_BUDGET) return path;
  }
  throw new Error("could not fit the GIF under 8 MB");
}

function exportPoster(master, time) {
  const path = join(DEMO, "poster.png");
  ffmpeg(["-ss", String(fixed(time)), "-i", master, "-frames:v", "1", path]);
  return path;
}

function reviewFrames(master, moments) {
  const folder = work("review");
  rmSync(folder, { recursive: true, force: true });
  mkdirSync(folder, { recursive: true });
  for (const [name, time] of Object.entries(moments)) {
    ffmpeg(["-ss", String(fixed(Math.max(0, time))), "-i", master, "-frames:v", "1", join(folder, `${name}.png`)]);
  }
  return folder;
}

function verifyDecodes(path) {
  const result = spawnSync("ffmpeg", ["-v", "error", "-i", path, "-f", "null", "-"], { encoding: "utf8" });
  if (result.status !== 0 || result.stderr.trim()) throw new Error(`${path} does not decode cleanly:
${result.stderr}`);
}

function removeIntermediates() {
  for (const name of ["footage.mkv", "master.mkv"]) rmSync(work(name), { force: true });
}

function report(path) {
  const result = spawnSync("ffprobe", ["-v", "error", "-show_entries", "stream=codec_type,codec_name,profile,pix_fmt,width,height,r_frame_rate:format=duration,size", "-of", "json", path], { encoding: "utf8" });
  const json = JSON.parse(result.stdout);
  const video = json.streams.find((stream) => stream.codec_type === "video");
  const audio = json.streams.filter((stream) => stream.codec_type === "audio").length;
  console.log(`${path}: ${video.codec_name} ${video.profile ?? ""} ${video.pix_fmt ?? ""} ${video.width}x${video.height} @ ${video.r_frame_rate}, ${Number(json.format.duration).toFixed(2)} s, ${(Number(json.format.size) / 1048576).toFixed(2)} MB, audio streams: ${audio}`);
}

function placeholderWindow(path, image) {
  const frame = { width: 1920, height: 1200, bar: 40 };
  const box = { cx: frame.width / 2, cy: frame.height / 2, hw: frame.width / 2, hh: frame.height / 2, r: 10 };
  const shell = paint(work("placeholder-shell.png"), frame.width, frame.height, (x, y) => {
    const inside = coverage(roundedBox(x, y, box));
    const color = y < frame.bar ? hex("#0B0D1A") : mixLinear(hex("#12152B"), hex("#1B1240"), smoothstep(0, frame.width, x + y));
    return [...color, inside];
  });
  const title = drawtext({ text: "Acme Cloud — Create project", weight: "regular", size: 15, color: "0xC8CCE0", x: 16, y: 26 });
  const label = drawtext({ text: "PLACEHOLDER FOOTAGE", weight: "bold", size: 96, color: "white@0.9", x: "(w-text_w)/2", y: "h/2" });
  const content = image
    ? `[1:v]scale=${frame.width}:${frame.height - frame.bar}[content];[0:v][content]overlay=0:${frame.bar}:format=rgb,format=rgba,${title}[shell]`
    : `[0:v]${title},${label}[shell]`;
  const inputs = image ? ["-i", shell, "-i", image] : ["-i", shell];
  ffmpeg([...inputs, "-filter_complex", content, "-map", "[shell]", "-frames:v", "1", path]);
  return path;
}

function makePlaceholder() {
  const timeline = {
    version: 1,
    placeholder: true,
    region: { x: 2, y: 2, width: 2016, height: 1296 },
    window: { x: 50, y: 50, width: 1920, height: 1200 },
    events: {
      read: { start: 0.75, end: 0.83, ms: 68 },
      fill: { start: 2.35, end: 7.5 },
      pick: { start: 7.55, end: 10.1 },
      create: { start: 10.15, end: 12.5 },
      shot: { start: 12.55, end: 12.65 },
    },
  };
  const timelinePath = work("placeholder-timeline.json");
  writeFileSync(timelinePath, JSON.stringify(timeline, null, 2));
  const images = typeof args["window-image"] === "string" ? args["window-image"].split(",") : [null];
  const windows = images.map((image, index) => placeholderWindow(work(`placeholder-window-${index}.png`), image));
  const span = 20 / windows.length;
  const inputs = windows.flatMap((path) => still(path, 20));
  const marks = Object.entries(timeline.events)
    .map(([name, event]) => drawtext({ text: name.toUpperCase(), weight: "bold", size: 54, color: "0xFFD54A", x: "w-text_w-120", y: 220 }) + `:enable='between(t,${event.start},${Math.max(event.end, event.start + 0.6)})'`)
    .join(",");
  const stamp = `drawtext=fontfile=${quoted(font("mono"))}:text='PLACEHOLDER %{pts\\:hms}':fontsize=34:fontcolor=0xFFD54A:x=120:y=200`;
  const chain = [`color=c=0x0B0D1A:s=2016x1296:r=${FPS}:d=20[bg0]`];
  windows.forEach((_, index) => {
    chain.push(`[${index}:v]format=rgba${index ? `,fade=t=in:st=${index * span}:d=0.4:alpha=1` : ""}[w${index}]`);
    chain.push(`[bg${index}][w${index}]overlay=48:48:enable='gte(t,${index * span})'[bg${index + 1}]`);
  });
  chain.push(`[bg${windows.length}]${stamp},${marks},format=yuv420p[out]`);
  const rawPath = work("placeholder-raw.mp4");
  ffmpeg([...inputs, "-/filter_complex", graph("placeholder", chain.join(";\n")), "-map", "[out]", "-t", "20", "-c:v", "libx264", "-crf", "16", "-preset", "veryfast", "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709", rawPath]);
  return { rawPath, timelinePath };
}

function main() {
  mkdirSync(WORK, { recursive: true });
  const inputs = args.placeholder ? makePlaceholder() : { rawPath: resolve(args.raw ?? join(DEMO, "raw.mp4")), timelinePath: resolve(args.timeline ?? join(DEMO, "timeline.json")) };
  for (const path of [inputs.rawPath, inputs.timelinePath, ICON]) {
    if (!existsSync(path)) throw new Error(`missing ${path}`);
  }
  const timeline = loadTimeline(inputs.timelinePath);
  const edit = loadEdit();
  console.log(`raw ${inputs.rawPath}\ntimeline ${inputs.timelinePath}${Object.keys(edit).length ? "\nedit.json applied" : ""}`);
  const intro = args["skip-cards"] && existsSync(work("intro.mkv")) ? work("intro.mkv") : renderIntro();
  const outro = args["skip-cards"] && existsSync(work("outro.mkv")) ? work("outro.mkv") : renderOutro();
  const footage = renderFootage(inputs.rawPath, timeline, edit);
  const master = assemble(intro, footage, outro);
  const at = (rawTime) => INTRO_SECONDS - CROSSFADE + footage.warp(rawTime);
  const mp4 = exportMp4(master);
  const poster = exportPoster(master, edit.poster ?? at(footage.events.fill.end - 0.5));
  const gif = args["skip-gif"] ? null : exportGif(master);
  const moments = { "1-intro": 1.6, "9-outro": INTRO_SECONDS + footage.duration + OUTRO_SECONDS - 2 * CROSSFADE - 0.4 };
  footage.captions.forEach((caption, index) => {
    moments[`${index + 2}-caption`] = INTRO_SECONDS - CROSSFADE + (caption.from + caption.to) / 2;
  });
  const review = reviewFrames(master, moments);
  for (const path of [mp4, gif].filter(Boolean)) {
    verifyDecodes(path);
    report(path);
  }
  if (!args["keep-work"]) removeIntermediates();
  console.log(`poster ${poster}\nreview frames in ${review}`);
}

main();
