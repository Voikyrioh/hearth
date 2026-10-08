import { describe, expect, it } from "vitest";
import { fr } from "./fr";

// HRT-39 (C34) : la page Sécurité parle en langage courant ; ces tournures de jargon ne reviennent pas.
describe("langage courant de la page Sécurité", () => {
  const all = JSON.stringify(fr.security);
  it.each(["à moitié reconnu", "seul signe"])("n'emploie pas « %s »", (jargon) => {
    expect(all).not.toContain(jargon);
  });
});

// HRT-47 (S4) : aucun texte de l'interface n'est copié de la machine de Voiky (nom ou adresse de la forge).
describe("textes d'exemple neutres", () => {
  const strings: string[] = [];
  const walk = (value: unknown) => {
    if (typeof value === "string") strings.push(value);
    else if (value && typeof value === "object") Object.values(value).forEach(walk);
  };
  walk(fr);
  it.each([/forge/i, /192\.168\.1\.20/, /voiky/i])("aucun texte ne contient %s", (pattern) => {
    expect(strings.filter((text) => pattern.test(text))).toEqual([]);
  });
});
