import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const tokens = readFileSync(resolve(process.cwd(), "src/styles/tokens.css"), "utf-8");

// HRT-37 : la poignée de défilement tient 3 pour 1 contre ce qu'il y a derrière (WCAG 1.4.11) et
// s'éclaircit au survol. Les couleurs sont lues dans `tokens.css`, pas recopiées.

function token(name: string): string {
  const match = new RegExp(`--${name}:s*([^;]+);`).exec(tokens);
  if (!match?.[1]) throw new Error(`jeton --${name} introuvable`);
  const value = match[1].trim();
  const alias = /^var\(--([\w-]+)\)$/.exec(value);
  return alias?.[1] ? token(alias[1]) : value;
}

function luminance(hex: string): number {
  const channel = (at: number) => {
    const c = Number.parseInt(hex.slice(at, at + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

function contrast(a: string, b: string): number {
  const [high = 0, low = 0] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (high + 0.05) / (low + 0.05);
}

describe("la poignée de défilement", () => {
  const thumb = token("scroll-thumb");
  const hover = token("scroll-thumb-hover");

  it("tient 3 pour 1 contre le fond de page, la barre latérale et une carte, au repos", () => {
    for (const surface of ["bg", "side", "card"]) {
      expect(contrast(thumb, token(surface)), surface).toBeGreaterThanOrEqual(3);
    }
  });

  it("est plus claire au survol, et plus contrastée", () => {
    expect(luminance(hover)).toBeGreaterThan(luminance(thumb));
    expect(contrast(hover, token("card"))).toBeGreaterThan(contrast(thumb, token("card")));
  });
});
