import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { ICO_FRAMES, TRAY_SIZES, TRAY_STATES } from "../../scripts/icon-spec.mjs";

// HRT-19 : l'identité visuelle livrée. Fichiers présents, dimensions exactes, cinq états distincts,
// couleurs des SVG dans la palette de la bible graphique (+ les trois couleurs d'état de l'interface
// pour l'icône de notification). Les images sont générées des SVG (`npm run build:icons`) ; leur accord
// avec les sources et leur contenu sont contrôlés dans `identity-sync.test.ts` (empreintes des sources).

const desktop = join(import.meta.dirname, "..", "..");
const icons = join(desktop, "src-tauri", "icons");
const source = join(icons, "source");

const BIBLE = ["#1c1518", "#2a2024", "#f6ece6", "#b3a19c", "#ff7b3d", "#ff4f7a", "#5fd0c0"];
const STATES = ["#7ed39a", "#ffc04d", "#ff5a5a"];

const bytes = (path: string) => readFileSync(path);
const svgFiles = (dir: string): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory()
      ? svgFiles(join(dir, entry.name))
      : entry.name.endsWith(".svg")
        ? [join(dir, entry.name)]
        : [],
  );

function pngSize(buffer: Buffer) {
  expect(buffer.subarray(0, 8).toString("hex")).toBe("89504e470d0a1a0a");
  return [buffer.readUInt32BE(16), buffer.readUInt32BE(20)];
}

describe("l'icône de l'application", () => {
  const ico = bytes(join(icons, "icon.ico"));

  it("a exactement les trames 16, 24, 32, 48, 64 et 256", () => {
    expect(ico.readUInt16LE(0)).toBe(0);
    expect(ico.readUInt16LE(2)).toBe(1);
    expect(ico.readUInt16LE(4)).toBe(ICO_FRAMES.length);
    const sizes = ICO_FRAMES.map((_, i) => {
      const w = ico.readUInt8(6 + 16 * i);
      return w === 0 ? 256 : w;
    });
    expect(sizes).toEqual([16, 24, 32, 48, 64, 256]);
    // Chaque trame est un PNG de la bonne taille, à l'endroit annoncé.
    ICO_FRAMES.forEach(({ size }, i) => {
      const length = ico.readUInt32LE(6 + 16 * i + 8);
      const offset = ico.readUInt32LE(6 + 16 * i + 12);
      expect(pngSize(ico.subarray(offset, offset + length))).toEqual([size, size]);
    });
  });

  it("a les PNG de la configuration Tauri aux bonnes dimensions", () => {
    const expected: Record<string, number> = {
      "32x32.png": 32,
      "64x64.png": 64,
      "128x128.png": 128,
      "128x128@2x.png": 256,
      "icon.png": 512,
    };
    for (const [name, size] of Object.entries(expected)) {
      expect(pngSize(bytes(join(icons, name))), name).toEqual([size, size]);
    }
    const conf = JSON.parse(readFileSync(join(desktop, "src-tauri", "tauri.conf.json"), "utf8"));
    for (const path of conf.bundle.icon as string[]) {
      expect(existsSync(join(desktop, "src-tauri", path)), path).toBe(true);
    }
  });

  it("n'a plus l'ancienne image de la zone de notification ni les dessins refusés", () => {
    expect(existsSync(join(icons, "tray.png"))).toBe(false);
    expect(existsSync(join(source, "hearth-logo-small.svg"))).toBe(false);
  });
});

describe("le bandeau de l'installateur", () => {
  it("est un BMP 24 bits de 164 x 314, déclaré dans la configuration NSIS", () => {
    const bmp = bytes(join(desktop, "src-tauri", "installer", "sidebar.bmp"));
    expect(bmp.subarray(0, 2).toString("ascii")).toBe("BM");
    expect(bmp.readInt32LE(18)).toBe(164);
    expect(bmp.readInt32LE(22)).toBe(314);
    expect(bmp.readUInt16LE(28)).toBe(24);
    expect(bmp.readUInt32LE(30)).toBe(0); // sans compression
    const conf = JSON.parse(readFileSync(join(desktop, "src-tauri", "tauri.conf.json"), "utf8"));
    expect(conf.bundle.windows.nsis.sidebarImage).toBe("installer/sidebar.bmp");
    expect(conf.bundle.windows.nsis.installerIcon).toBe("icons/icon.ico");
  });
});

describe("l'icône de la zone de notification", () => {
  const file = (state: string, size: number) => join(icons, "tray", `tray-${state}-${size}.png`);

  it("a une image par état et par taille, aux bonnes dimensions", () => {
    expect(TRAY_STATES).toHaveLength(6); // cinq états du lien + aucun serveur
    for (const state of TRAY_STATES) {
      for (const size of TRAY_SIZES) {
        expect(pngSize(bytes(file(state, size))), `${state} ${size}`).toEqual([size, size]);
      }
    }
  });

  it("n'a jamais deux images identiques à une même taille", () => {
    for (const size of TRAY_SIZES) {
      const seen = TRAY_STATES.map((state) => bytes(file(state, size)).toString("base64"));
      expect(new Set(seen).size, `taille ${size}`).toBe(TRAY_STATES.length);
    }
  });

  it("a un SVG source par état", () => {
    for (const state of TRAY_STATES) {
      expect(existsSync(join(source, "tray", `tray-${state}.svg`)), state).toBe(true);
    }
  });
});

describe("les SVG", () => {
  const all = svgFiles(source);

  it("existent : logo, icônes, trois monochromes, six états", () => {
    for (const name of [
      "logo.svg",
      "app-icon.svg",
      "app-icon-16.svg",
      "mark-16.svg",
      "logo-mono.svg",
      "logo-mono-clair.svg",
      "logo-mono-sombre.svg",
    ]) {
      expect(existsSync(join(source, name)), name).toBe(true);
    }
  });

  it("n'utilisent que les couleurs de la bible (et les trois couleurs d'état pour l'icône de notification)", () => {
    expect(all.length).toBeGreaterThanOrEqual(13);
    for (const path of all) {
      const text = readFileSync(path, "utf8");
      const colors = (text.match(/#[0-9a-fA-F]{3,8}\b/g) ?? []).map((c) => c.toLowerCase());
      const allowed = path.includes("tray-") ? [...BIBLE, ...STATES] : BIBLE;
      for (const color of colors) expect(allowed, `${path} : ${color}`).toContain(color);
      expect(text, `${path} : image matricielle intégrée`).not.toMatch(/<image\b|data:image/);
    }
  });

  it("gardent les jetons du thème d'accord avec la bible", () => {
    const tokens = readFileSync(join(desktop, "src", "styles", "tokens.css"), "utf8");
    for (const color of [...BIBLE, ...STATES]) expect(tokens.toLowerCase()).toContain(color);
  });
});
