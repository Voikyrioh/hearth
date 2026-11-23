import { describe, expect, it } from "vitest";
import {
  classifyLine,
  normalizeLine,
  playerOf,
  sensitiveTitle,
  signedClass,
  verbOf,
} from "./consoleLine";

// Le miroir de `hearth_proto::console` : les mêmes vecteurs que ses tests Rust (forme d'une ligne, verbe jugé, classe).
const policy = {
  refused: [
    { verb: "stop", reason: "use_hearth_stop" },
    { verb: "restart", reason: "use_hearth_restart" },
  ],
  sensitive: [
    {
      verb: "op",
      title: "Donner les droits d'opérateur à {joueur} ?",
      sentence: "...",
      target: "first_arg" as const,
    },
    {
      verb: "whitelist",
      title: "Changer la liste blanche ?",
      sentence: "...",
      target: "none" as const,
    },
  ],
};

describe("la forme d'une ligne", () => {
  it("retire les espaces de tête et de fin et UNE barre oblique de tête", () => {
    expect(normalizeLine("  /time set day ")).toEqual({ ok: true, line: "time set day" });
    expect(normalizeLine("//x")).toEqual({ ok: true, line: "/x" });
    expect(normalizeLine("/ say a")).toEqual({ ok: true, line: "say a" });
  });

  it("refuse vide, trop long, et tout caractère de contrôle ou invisible", () => {
    expect(normalizeLine("")).toEqual({ ok: false, error: "empty" });
    expect(normalizeLine(" / ")).toEqual({ ok: false, error: "empty" });
    expect(normalizeLine("a".repeat(256))).toMatchObject({ ok: true });
    expect(normalizeLine("a".repeat(257))).toEqual({ ok: false, error: "too_long" });
    expect(normalizeLine("😀".repeat(256))).toMatchObject({ ok: true });
    for (const bad of ["say a\nstop", "a\tb", "a\u0000b", "a​b", "a‮b", "a\u0085b"]) {
      expect(normalizeLine(bad), JSON.stringify(bad)).toEqual({ ok: false, error: "control_char" });
    }
  });
});

describe("la classe d'une commande", () => {
  it("le verbe est le premier mot en minuscules ASCII, sans le préfixe minecraft:", () => {
    expect(verbOf("OP Bram")).toBe("op");
    expect(verbOf("minecraft:stop")).toBe("stop");
    expect(verbOf("MINECRAFT:Stop now")).toBe("stop");
  });

  it("refusée, sensible ou libre selon la fiche, jamais selon le client", () => {
    expect(classifyLine(policy, "stop")).toMatchObject({
      kind: "refused",
      reason: "use_hearth_stop",
    });
    expect(classifyLine(policy, "minecraft:restart")).toMatchObject({ kind: "refused" });
    expect(classifyLine(policy, "op Bram")).toMatchObject({ kind: "sensitive", player: "Bram" });
    expect(classifyLine(policy, "whitelist add Bram")).toMatchObject({
      kind: "sensitive",
      player: null,
    });
    expect(classifyLine(policy, "time set day")).toEqual({ kind: "free" });
    expect(classifyLine({ refused: [], sensitive: [] }, "stop")).toEqual({ kind: "free" });
  });

  it("le code signé : 1 libre, 2 sensible, rien pour une commande refusée", () => {
    expect(signedClass(classifyLine(policy, "list"))).toBe(1);
    expect(signedClass(classifyLine(policy, "op Bram"))).toBe(2);
    expect(signedClass(classifyLine(policy, "stop"))).toBeNull();
  });

  it("le joueur visé n'est repris que s'il a la forme d'un pseudo", () => {
    expect(playerOf("op Bram")).toBe("Bram");
    expect(playerOf("op   Bram")).toBe("Bram");
    expect(playerOf("op @a")).toBeNull();
    expect(playerOf("op")).toBeNull();
    expect(playerOf(`op ${"a".repeat(17)}`)).toBeNull();
  });

  it("le titre de la confirmation remplit {joueur} ou le retire sans casser la phrase", () => {
    const title = "Donner les droits d'opérateur à {joueur} ?";
    expect(sensitiveTitle(title, "Bram")).toBe("Donner les droits d'opérateur à Bram ?");
    expect(sensitiveTitle(title, null)).toBe("Donner les droits d'opérateur ?");
    expect(sensitiveTitle("Bannir {joueur} ?", null)).toBe("Bannir ?");
  });
});
