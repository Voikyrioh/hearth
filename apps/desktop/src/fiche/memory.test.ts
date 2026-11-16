import { describe, expect, it } from "vitest";
import { GIB } from "@/resources";
import { memorySentence } from "./memory";

const ADVICE = { recommendedBytes: 4 * GIB, evidence: { kind: "measured" } } as const;

describe("la mémoire conseillée en une phrase (BR-FICHE-064)", () => {
  it("mesurée, machine connue ou non", () => {
    expect(memorySentence(ADVICE, 64 * GIB, "forge", "card")).toBe(
      "4 Go de mémoire conseillés, forge en a 64 Go",
    );
    expect(memorySentence(ADVICE, null, "forge", "card")).toBe("4 Go de mémoire conseillés");
    expect(memorySentence(ADVICE, 64 * GIB, "forge", "fact")).toBe(
      "4 Go de mémoire, forge en a 64 Go",
    );
  });

  it("mesurée au repos : le dit, avec la machine si elle est connue", () => {
    const idle = { recommendedBytes: 2 * GIB, evidence: { kind: "measured_idle" } } as const;
    expect(memorySentence(idle, 8 * GIB, "forge", "card")).toBe(
      "2 Go de mémoire mesurés sans joueur, forge en a 8 Go",
    );
    expect(memorySentence(idle, null, "forge", "fact")).toBe("2 Go de mémoire mesurés sans joueur");
  });

  it("chiffre de l'éditeur : jamais la machine", () => {
    const publisher = {
      recommendedBytes: 16 * GIB,
      evidence: { kind: "publisher", url: "https://exemple.test" },
    } as const;
    expect(memorySentence(publisher, 64 * GIB, "forge", "card")).toBe(
      "16 Go de mémoire selon l'éditeur, non mesurés par Hearth",
    );
  });

  it("sans chiffre, sans preuve ou preuve inconnue : non vérifiée", () => {
    const text = "Mémoire nécessaire non vérifiée par Hearth";
    expect(memorySentence(null, 8 * GIB, "forge", "card")).toBe(text);
    expect(memorySentence(undefined, null, "forge", "card")).toBe(text);
    expect(
      memorySentence(
        { recommendedBytes: GIB, evidence: { kind: "unmeasured" } },
        8 * GIB,
        "forge",
        "card",
      ),
    ).toBe(text);
    expect(
      memorySentence(
        { recommendedBytes: GIB, evidence: { kind: "unknown" } },
        8 * GIB,
        "forge",
        "card",
      ),
    ).toBe(text);
  });
});
