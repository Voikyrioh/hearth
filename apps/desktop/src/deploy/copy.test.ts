import { describe, expect, it } from "vitest";
import { fr } from "@/i18n/fr";

// Les règles de rédaction des textes de déploiement (HRT-69) : vouvoiement et impératif, phrases complètes, jamais
// de tiret cadratin, aucun nom interne du moteur de conteneurs, jamais « isolé » ni « sûr » (BR-DEPLOY-036).
const strings: [string, string][] = [];
function walk(value: unknown, path: string) {
  if (typeof value === "string") strings.push([path, value]);
  else if (value && typeof value === "object") {
    for (const [key, inner] of Object.entries(value)) walk(inner, `${path}.${key}`);
  }
}
walk(fr.deploy, "deploy");

describe("les textes de déploiement", () => {
  it("existent et sont nombreux", () => {
    expect(strings.length).toBeGreaterThan(150);
  });

  it.each([
    ["tiret cadratin", /—|–/],
    ["Docker", /docker/i],
    [
      "tutoiement",
      /(?<![\p{L}'])(tu|ton|ta|tes|toi|te|t)(?![\p{L}])|(?<![\p{L}])(vérifie|choisis|attends|réessaie|corrige|renseigne|utilise|ne mets pas)(?![\p{L}])/iu,
    ],
    ["isolé ou sûr", /(?<![\p{L}])(isolé|isolée|sûr|sûre|sécurisé)(?![\p{L}])/iu],
  ])("aucun ne contient %s", (_name, pattern) => {
    const found = strings.filter(([, text]) => pattern.test(text)).map(([path]) => path);
    expect(found).toEqual([]);
  });

  it("aucun ne dit le nom interne d'un conteneur, d'un volume ou d'une empreinte hors de la phrase de la spec", () => {
    const found = strings
      .filter(([path, text]) => /\bsha256\b/.test(text) && path !== "deploy.imageDigest")
      .map(([path]) => path);
    expect(found).toEqual([]);
  });
});
