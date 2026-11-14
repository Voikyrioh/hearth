import { describe, expect, it } from "vitest";
import { fr } from "@/i18n/fr";

// Les règles de rédaction des textes de suppression (HRT-86) : vouvoiement et impératif, phrases complètes, jamais de
// tiret cadratin, jamais « Hearth ne la garde pas » (BR-DEL-004), jamais de nom interne du moteur de conteneurs.
const strings: [string, string][] = [];
function walk(value: unknown, path: string) {
  if (typeof value === "string") strings.push([path, value]);
  else if (value && typeof value === "object") {
    for (const [key, inner] of Object.entries(value)) walk(inner, `${path}.${key}`);
  }
}
walk(fr.serviceDelete, "serviceDelete");

describe("les textes de suppression", () => {
  it("existent et sont nombreux", () => {
    expect(strings.length).toBeGreaterThan(80);
  });

  it.each([
    ["tiret cadratin", /—|–/],
    ["Docker", /docker/i],
    ["Hearth ne la garde pas", /ne la garde pas/i],
    [
      "tutoiement",
      /(?<![\p{L}'])(tu|ton|ta|tes|toi|te|t)(?![\p{L}])|(?<![\p{L}])(vérifie|choisis|attends|réessaie|corrige|retape|crée|passe|décoche)(?![\p{L}])/iu,
    ],
  ])("aucun ne contient %s", (_name, pattern) => {
    const found = strings.filter(([, text]) => pattern.test(text)).map(([path]) => path);
    expect(found).toEqual([]);
  });

  it("dit que la configuration est effacée avec le service (BR-DEL-004)", () => {
    expect(fr.serviceDelete.effect).toContain(
      "Sa configuration (variables, ports, commande) sera effacée avec lui.",
    );
  });

  it("les cases et le bouton portent les libellés de la spécification (BR-DEL-005)", () => {
    expect(fr.serviceDelete.button).toBe("Supprimer ce service");
    expect(fr.serviceDelete.dataBox).toBe("Supprimer aussi les données");
    expect(fr.serviceDelete.backupsBox).toBe("Supprimer aussi les sauvegardes de ce service");
    expect(fr.serviceDelete.submit).toBe("Supprimer le service");
  });
});
