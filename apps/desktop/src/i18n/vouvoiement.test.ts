import { describe, expect, it } from "vitest";
import { fr } from "./fr";

// Les textes de l'assistant « Relier un serveur » et ceux du lot Catalogue et ajout d'un serveur de jeu sont au vouvoiement
// et à l'impératif (HRT-248, règle de Voiky du 14/10) : aucun tutoiement (« tu », « ton », « ta », « tes », « te », « toi »,
// impératif du tutoiement), aucun tiret cadratin ni demi-cadratin. Le reste de l'application suit avec HRT-250.
const TU_FORMS =
  /\b(tu|toi|ton|ta|tes|te)\b|\b(vérifie|compare|connecte|attends|essaie|réessaie|ajoute)-toi\b|(?:^|\. )(Vérifie|Compare|Lance|Attends|Essaie|Réessaie|Ajoute)\b/;

function flatten(tree: unknown, path: string, out: Array<[string, string]>) {
  if (typeof tree === "string") out.push([path, tree]);
  else if (tree && typeof tree === "object") {
    for (const [key, value] of Object.entries(tree))
      flatten(value, path ? `${path}.${key}` : key, out);
  }
}

function texts(root: keyof typeof fr): Array<[string, string]> {
  const out: Array<[string, string]> = [];
  flatten(fr[root], root, out);
  return out;
}

describe("le vouvoiement de l'assistant et du lot catalogue", () => {
  it("aucune clé de `connect` ne tutoie", () => {
    for (const [path, text] of texts("connect")) {
      expect(text, path).not.toMatch(TU_FORMS);
    }
  });

  it("l'exemple d'identifiant est au vouvoiement", () => {
    expect(fr.accounts.usernamePlaceholder).toBe("votre-identifiant");
  });

  it("`addGame` et `catalogue` ne tutoient pas", () => {
    for (const root of ["addGame", "catalogue"] as const) {
      for (const [path, text] of texts(root)) {
        expect(text, path).not.toMatch(TU_FORMS);
      }
    }
  });

  it("aucun tiret cadratin dans tout fr.ts", () => {
    const all: Array<[string, string]> = [];
    flatten(fr, "", all);
    for (const [path, text] of all) {
      expect(text, path).not.toContain("—");
    }
  });
});
