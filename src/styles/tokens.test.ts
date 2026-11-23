import { readFileSync } from "node:fs";
import { resolve } from "node:path";

/** Parse `--name: value;` declarations of one CSS block (`:root {` or `:root.daylight {`). */
function block(css: string, selector: string): Record<string, string> {
  const start = css.indexOf(`${selector} {`);
  if (start < 0) throw new Error(`no ${selector} block`);
  const body = css.slice(start, css.indexOf("}", start));
  const out: Record<string, string> = {};
  for (const m of body.matchAll(/--([\w-]+):\s*([^;]+);/g)) out[m[1]] = m[2].trim();
  return out;
}

type RGBA = [number, number, number, number];

export function parseColor(v: string): RGBA {
  const hex = v.match(/^#([0-9a-f]{6})$/i);
  if (hex) {
    const n = parseInt(hex[1], 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 1];
  }
  const rgba = v.match(/^rgba?\(([^)]+)\)$/);
  if (rgba) {
    const [r, g, b, a = "1"] = rgba[1].split(",").map((x) => x.trim());
    return [Number(r), Number(g), Number(b), Number(a)];
  }
  throw new Error(`unsupported color ${v}`);
}

/** `top` composited over an opaque `bottom`. */
function over(top: RGBA, bottom: RGBA): RGBA {
  const a = top[3];
  return [0, 1, 2].map((i) => top[i] * a + bottom[i] * (1 - a)).concat(1) as RGBA;
}

function luminance([r, g, b]: RGBA): number {
  const lin = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

export function contrast(a: RGBA, b: RGBA): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const css = readFileSync(resolve(__dirname, "globals.css"), "utf8");
const dark = block(css, ":root");
const themes = { dark, daylight: { ...dark, ...block(css, ":root.daylight") } };

/**
 * Text/background pairs that occur in the UI. A background may be layered: `[layer, base]` means `layer`
 * (often translucent) drawn over `base`.
 */
const PAIRS: { text: string; bg: string[] }[] = [
  ...["bg", "bg-sidebar", "surface", "surface-raised"].flatMap((bg) => [
    { text: "text", bg: [bg] },
    { text: "text-muted", bg: [bg] },
  ]),
  { text: "accent-fg", bg: ["accent"] }, // primary buttons, active tabs
  { text: "accent-2", bg: ["accent-2-soft", "surface"] }, // status chips
  { text: "accent", bg: ["accent-soft", "surface"] }, // highlighted rows
  { text: "danger", bg: ["surface"] }, // error messages
  { text: "danger", bg: ["surface-raised"] },
  { text: "warn", bg: ["surface"] },
  { text: "success", bg: ["surface"] },
];

describe("design tokens (UI-7, P1)", () => {
  for (const [name, t] of Object.entries(themes)) {
    for (const p of PAIRS) {
      it(`${name}: --${p.text} on ${p.bg.map((b) => `--${b}`).join(" over ")} meets WCAG AA (4.5:1)`, () => {
        const layers = p.bg.map((b) => parseColor(t[b]));
        // Composite from the bottom layer up; the bottom must be opaque.
        let bg = layers[layers.length - 1];
        for (let i = layers.length - 2; i >= 0; i--) bg = over(layers[i], bg);
        const fg = over(parseColor(t[p.text]), bg);
        expect(contrast(fg, bg)).toBeGreaterThanOrEqual(4.5);
      });
    }
  }

  it("contrast math matches known values", () => {
    expect(contrast(parseColor("#000000"), parseColor("#ffffff"))).toBeCloseTo(21, 5);
    expect(contrast(parseColor("#777777"), parseColor("#ffffff"))).toBeCloseTo(4.48, 2);
  });
});

describe("native palette (UI-10, P2)", () => {
  const swift = readFileSync(resolve(__dirname, "../../native/bob-native/Sources/BobNativeCore/ChipPanel.swift"), "utf8");
  const palette: Record<string, string> = {};
  for (const m of swift.matchAll(
    /static let (\w+) = NSColor\(srgbRed: 0x([0-9A-F]{2}) \/ 255, green: 0x([0-9A-F]{2}) \/ 255, blue: 0x([0-9A-F]{2}) \/ 255/gi,
  )) {
    palette[m[1]] = `#${m[2]}${m[3]}${m[4]}`.toLowerCase();
  }
  // The native overlays (chip, action menu, orb) are always dark, so they mirror the dark tokens.
  const mapping: Record<string, string> = { ectoplasm: "accent", midnight: "bg", pumpkin: "accent-2" };

  it("parses every Palette color", () => {
    expect(Object.keys(palette).sort()).toEqual(Object.keys(mapping).sort());
  });

  for (const [swiftName, token] of Object.entries(mapping)) {
    it(`Palette.${swiftName} equals --${token}`, () => {
      expect(palette[swiftName]).toBe(dark[token].toLowerCase());
    });
  }
});
