import { describe, expect, it } from "vitest";
import { CONTROL_MARK, safeText } from "./text";

// La fonction UNIQUE qui rend inoffensif un texte venu du serveur (liste et détail des services, journal).
const at = (code: number) => String.fromCharCode(code);

describe("safeText", () => {
  it("remplace chaque caractère bidirectionnel ou invisible par un repère visible, jamais en silence", () => {
    const hostile = [
      0x202a, 0x202b, 0x202c, 0x202d, 0x202e, 0x2066, 0x2067, 0x2068, 0x2069, 0x200e, 0x200f,
      0x061c,
    ]
      .concat([
        0x200b, 0x200c, 0x200d, 0x2060, 0xfeff, 0xad, 0x034f, 0x3164, 0xffa0, 0x2028, 0x2029,
      ])
      .map(at);
    for (const char of hostile) {
      expect(safeText(`a${char}b`)).toBe(`a${CONTROL_MARK}b`);
    }
    // La longueur ne ment pas : autant de repères que de caractères remplacés.
    expect(safeText(hostile.join("")).length).toBe(hostile.length);
  });

  it("remplace les caractères de contrôle, la tabulation et le saut de ligne compris", () => {
    expect(safeText("a\tb\nc\u0000d\u007fe\u0085f")).toBe(
      `a${CONTROL_MARK}b${CONTROL_MARK}c${CONTROL_MARK}d${CONTROL_MARK}e${CONTROL_MARK}f`,
    );
  });

  it("garde le texte ordinaire tel quel : accents, emoji, caractères non latins", () => {
    for (const text of ["héhé : œuvre", "日本語", "mot de passe 🔒", "a  b"]) {
      expect(safeText(text)).toBe(text);
    }
    expect(safeText(null)).toBe("");
    expect(safeText(undefined)).toBe("");
  });

  it("avec keepLines : la tabulation et le saut de ligne restent, tout le reste est remplacé", () => {
    const text = `ligne 1\r\nligne 2\tfin${at(0x202e)}${at(0x0)}${at(0x0d)}`;
    expect(safeText(text, true)).toBe(
      `ligne 1\nligne 2\tfin${CONTROL_MARK}${CONTROL_MARK}${CONTROL_MARK}`,
    );
  });
});
