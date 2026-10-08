import { describe, expect, it } from "vitest";
import { fr } from "./fr";

// HRT-39 (C34) : la page Sécurité parle en langage courant ; ces tournures de jargon ne reviennent pas.
describe("langage courant de la page Sécurité", () => {
  const all = JSON.stringify(fr.security);
  it.each(["à moitié reconnu", "seul signe"])("n'emploie pas « %s »", (jargon) => {
    expect(all).not.toContain(jargon);
  });
});
