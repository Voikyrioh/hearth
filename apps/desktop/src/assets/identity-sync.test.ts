import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { ICO_FRAMES, RENDER_VERSION, TRAY_SIZES, TRAY_STATES } from "../../scripts/icon-spec.mjs";
import { ILLUSTRATIONS } from "./illustrations";
import { SCREEN_ILLUSTRATIONS } from "./illustrations/screens";
import { decodePng } from "./png-decode";

// HRT-19 : les images générées sont bien celles de leurs sources, et ne sont pas vides.
// `npm run build:icons` écrit `icons/sources.sha256.json` : empreinte de chaque source, version du
// rendu, empreinte de chaque image produite (le fichier commité, jamais un nouveau rendu : Chromium
// n'a pas à être stable d'une version à l'autre). Source changée sans régénération, image retouchée
// ou échangée à la main : le test échoue.

const desktop = join(import.meta.dirname, "..", "..");
const icons = join(desktop, "src-tauri", "icons");
const source = join(icons, "source");
const bytes = (path: string) => readFileSync(path);
const tokens = readFileSync(join(desktop, "src", "styles", "tokens.css"), "utf8");
const token = (name: string) =>
  tokens.match(new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{6})`))?.[1]?.toLowerCase() ?? "";

describe("l'accord entre les sources et les images générées", () => {
  const fingerprints = JSON.parse(readFileSync(join(icons, "sources.sha256.json"), "utf8")) as {
    sources: Record<string, string>;
    palette: Record<string, string>;
    renderVersion: number;
    outputs: Record<string, { sha256: string; from: string[] }>;
  };

  it("a la version du rendu du script", () => {
    expect(fingerprints.renderVersion, "RENDER_VERSION a changé : npm run build:icons").toBe(
      RENDER_VERSION,
    );
  });

  it("a, pour chaque source, l'empreinte relevée à la dernière génération", () => {
    for (const [path, expected] of Object.entries(fingerprints.sources)) {
      const text = readFileSync(join(desktop, path), "utf8").replaceAll("\r\n", "\n");
      const actual = createHash("sha256").update(text).digest("hex");
      expect(actual, `${path} a changé : régénère avec npm run build:icons`).toBe(expected);
    }
  });

  it("a, pour chaque image produite, l'empreinte du fichier relevée à la génération", () => {
    for (const [output, { sha256 }] of Object.entries(fingerprints.outputs)) {
      const data = readFileSync(join(desktop, output));
      const text = output.endsWith(".svg") ? data.toString("utf8").replaceAll("\r\n", "\n") : data;
      const actual = createHash("sha256").update(text).digest("hex");
      expect(actual, `${output} retouchée ou échangée à la main : npm run build:icons`).toBe(
        sha256,
      );
    }
  });

  it("a le bandeau aux couleurs des jetons du thème", () => {
    for (const name of ["bg", "card", "ac"]) {
      expect(fingerprints.palette[name], `--${name} a changé : npm run build:icons`).toBe(
        token(name),
      );
    }
  });

  it("a une image pour chaque sortie annoncée, et chaque dépendance est une source relevée", () => {
    expect(Object.keys(fingerprints.outputs).length).toBeGreaterThan(30);
    for (const [output, { from }] of Object.entries(fingerprints.outputs)) {
      expect(existsSync(join(desktop, output)), output).toBe(true);
      for (const dependency of from) {
        const known = dependency === "palette" || dependency in fingerprints.sources;
        expect(known, `${output} : ${dependency}`).toBe(true);
      }
    }
  });
});

describe("le contenu des images", () => {
  const stats = (buffer: Buffer) => {
    const { rgba } = decodePng(buffer);
    const colors = new Set<string>();
    let opaque = 0;
    for (let i = 0; i < rgba.length; i += 4) {
      if ((rgba[i + 3] ?? 0) > 0) opaque += 1;
      colors.add(`${rgba[i]},${rgba[i + 1]},${rgba[i + 2]},${rgba[i + 3]}`);
    }
    return { rgba, opaque, distinct: colors.size };
  };
  const has = (rgba: Uint8Array, hex: string) => {
    const want = [1, 3, 5].map((i) => Number.parseInt(hex.slice(i, i + 2), 16));
    for (let i = 0; i < rgba.length; i += 4) {
      const close = want.every((v, k) => Math.abs((rgba[i + k] ?? 0) - v) <= 6);
      if ((rgba[i + 3] ?? 0) >= 64 && close) return true;
    }
    return false;
  };

  it("n'a aucune image de l'application vide ou unie, la braise y est", () => {
    for (const name of ["32x32.png", "64x64.png", "128x128.png", "128x128@2x.png", "icon.png"]) {
      const found = stats(bytes(join(icons, name)));
      expect(found.opaque, name).toBeGreaterThan(0);
      expect(found.distinct, name).toBeGreaterThan(1);
      expect(has(found.rgba, token("ac")), `${name} : braise`).toBe(true);
    }
    const ico = bytes(join(icons, "icon.ico"));
    ICO_FRAMES.forEach(({ size }, i) => {
      const length = ico.readUInt32LE(6 + 16 * i + 8);
      const offset = ico.readUInt32LE(6 + 16 * i + 12);
      const found = stats(ico.subarray(offset, offset + length));
      expect(found.distinct, `trame ${size}`).toBeGreaterThan(1);
      expect(has(found.rgba, token("ac")), `trame ${size} : braise`).toBe(true);
    });
  });

  it("a, dans chaque image de notification, la couleur de son état", () => {
    const expected: Record<string, string> = {
      "aucun-serveur": token("tx2"),
      connecte: token("ac"),
      reconnexion: token("warn"),
      "hors-ligne": token("crit"),
      "session-expiree": token("warn"),
      "acces-revoque": token("crit"),
    };
    for (const state of TRAY_STATES) {
      for (const size of TRAY_SIZES) {
        const found = stats(bytes(join(icons, "tray", `tray-${state}-${size}.png`)));
        expect(found.opaque, `${state} ${size}`).toBeGreaterThan(size);
        expect(found.distinct, `${state} ${size}`).toBeGreaterThan(1);
        expect(has(found.rgba, expected[state] ?? ""), `${state} ${size} : couleur`).toBe(true);
      }
    }
  });

  it("n'a pas un bandeau vide ou uni", () => {
    const bmp = bytes(join(desktop, "src-tauri", "installer", "sidebar.bmp"));
    const colors = new Set<number>();
    for (let at = 54; at + 2 < bmp.length; at += 3) colors.add(bmp.readUIntBE(at, 3));
    expect(colors.size).toBeGreaterThan(8);
  });
});

describe("le logo n'a qu'une source de tracé", () => {
  const attr = (tag: string, name: string) =>
    tag.match(new RegExp(String.raw`\s${name}="([^"]*)"`))?.[1];
  // Pour chaque tracé : le dessin (`d`), le trait (épaisseur, extrémité) et si la forme est pleine.
  const drawing = (svg: string) =>
    [...svg.matchAll(/<path\b[^>]*>/gs)].map((m) => ({
      d: attr(m[0], "d"),
      strokeWidth: attr(m[0], "stroke-width"),
      linecap: attr(m[0], "stroke-linecap"),
      filled: attr(m[0], "fill") !== "none",
    }));
  const viewBox = (svg: string) => attr(svg.match(/<svg\b[^>]*>/s)?.[0] ?? "", "viewBox");
  const master = readFileSync(join(source, "logo.svg"), "utf8");

  it("est le même dans les variantes, l'icône, le favicon et HLogo", () => {
    expect(drawing(master)).toHaveLength(2);
    expect(drawing(master)[0]?.strokeWidth).toBe("46");
    const copies: [string, boolean][] = [
      [join(source, "logo-mono.svg"), true],
      [join(source, "logo-mono-clair.svg"), true],
      [join(source, "logo-mono-sombre.svg"), true],
      [join(desktop, "public", "favicon.svg"), true],
      [join(desktop, "src", "components", "atoms", "HLogo.vue"), true],
      // L'icône d'application pose le logo dans un carré (zone de vue et transformation à elle).
      [join(source, "app-icon.svg"), false],
    ];
    for (const [file, sameViewBox] of copies) {
      const text = readFileSync(file, "utf8");
      expect(drawing(text), file).toEqual(drawing(master));
      if (sameViewBox) expect(viewBox(text), `${file} : zone de vue`).toBe(viewBox(master));
    }
  });
});

describe("la table des illustrations par écran", () => {
  it("ne cite que des illustrations qui existent, pour les quatre écrans", () => {
    for (const [screen, name] of Object.entries(SCREEN_ILLUSTRATIONS)) {
      if (name !== null) expect(ILLUSTRATIONS, screen).toHaveProperty(name);
    }
    expect(Object.keys(SCREEN_ILLUSTRATIONS).sort()).toEqual([
      "accounts",
      "journal",
      "offline",
      "welcome",
    ]);
  });
});
