import { describe, expect, it } from "vitest";
import { fr } from "@/i18n/fr";

// Les règles de rédaction des textes du Catalogue et de l'installation (HRT-102) : vouvoiement et impératif, phrases
// complètes, jamais de tiret cadratin, aucun nom interne du moteur de conteneurs, jamais « isolé » ni « sûr »
// (BR-FICHE-036, BR-GRANT-022). Les mêmes règles que celles des textes de déploiement (HRT-69).
const strings: [string, string][] = [];
function walk(value: unknown, path: string) {
  if (typeof value === "string") strings.push([path, value]);
  else if (value && typeof value === "object") {
    for (const [key, inner] of Object.entries(value)) walk(inner, `${path}.${key}`);
  }
}
walk(fr.fiche, "fiche");
walk(fr.catalogue, "catalogue");

describe("les textes du Catalogue et de l'installation", () => {
  it("existent et sont nombreux", () => {
    expect(strings.length).toBeGreaterThan(200);
  });

  it.each([
    ["tiret cadratin", /—|–/],
    ["Docker", /docker/i],
    [
      "tutoiement",
      /(?<![\p{L}'])(tu|ton|ta|tes|toi|te|t)(?![\p{L}])|(?<![\p{L}])(vérifie|choisis|attends|réessaie|corrige|renseigne|utilise|laisse|ouvre|mets|libère)(?![\p{L}])/iu,
    ],
    ["isolé ou sûr", /(?<![\p{L}])(isolé|isolée|sûr|sûre|sécurisé|protégé)(?![\p{L}])/iu],
    ["une adresse ou un nom propre au jeu", /terraria|minecraft|valheim|7 days|\bhttps?:\/\//i],
  ])("aucun ne contient %s", (_name, pattern) => {
    const found = strings.filter(([, text]) => pattern.test(text)).map(([path]) => path);
    expect(found).toEqual([]);
  });

  it("ne cite jamais une empreinte hors des deux phrases qui la nomment", () => {
    const found = strings
      .filter(([path, text]) => /\bsha256\b/.test(text) && path !== "fiche.imageDigest")
      .map(([path]) => path);
    expect(found).toEqual([]);
  });
});
