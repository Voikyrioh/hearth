import { describe, expect, it } from "vitest";
import { isHearthUsername, isSystemAccountName } from "./name";

// HRT-74 S7 et HRT-78 : DEUX espaces de noms, une validation chacun, avant tout affichage (phrase, titre,
// info-bulle, nom accessible, bloc à recopier). Un nom qui ne la passe pas n'est montré nulle part.
const HOSTILE = [
  "",
  "a".repeat(33),
  "a".repeat(5000),
  "hearth user",
  " hearth",
  "hearth ",
  "hearth\tx",
  "hearth;id",
  "hearth && reboot",
  "hearth|id",
  "hearth>x",
  "$(id)",
  "`id`",
  "hearth$",
  "hearth\nsudo rm -rf /",
  "hearth\r\n",
  "hearth\n",
  "Hearth",
  "-hearth",
  "--grant",
  "../root",
  "he'arth",
  'he"arth',
  "héarth",
  "ｈearth",
  "hearth‮gnp",
  "⁧hearth⁩",
  "hearth​",
  "hearth\u0000",
  "<b>hearth</b>",
];

describe("isSystemAccountName (compte sous lequel tourne l'agent)", () => {
  it("accepte un nom POSIX sobre, y compris court", () => {
    for (const name of [
      "hearth",
      "_hearth",
      "hearth-agent",
      "svc_1",
      "pi",
      "a",
      "_",
      "a".repeat(32),
    ]) {
      expect(isSystemAccountName(name), name).toBe(true);
    }
  });

  it("refuse un nom qui ne commence pas par une lettre ou `_`", () => {
    for (const name of ["1hearth", "-hearth", "3615paul"]) {
      expect(isSystemAccountName(name), name).toBe(false);
    }
  });

  it("refuse tout ce que le shell, l'oeil ou la longueur peuvent détourner", () => {
    for (const name of HOSTILE) {
      expect(isSystemAccountName(name), JSON.stringify(name.slice(0, 40))).toBe(false);
    }
  });
});

describe("isHearthUsername (identifiant d'un compte Hearth, account_rules.rs)", () => {
  it("accepte 3 à 32 caractères de [a-z0-9_-], même commençant par un chiffre", () => {
    for (const name of ["marie", "abc", "3615paul", "123", "_svc", "svc-1", "a".repeat(32)]) {
      expect(isHearthUsername(name), name).toBe(true);
    }
  });

  it("refuse moins de 3 caractères", () => {
    for (const name of ["", "a", "pi", "ab"]) {
      expect(isHearthUsername(name), name).toBe(false);
    }
  });

  it("refuse un tiret initial (lu comme une option de la ligne de commande)", () => {
    for (const name of ["-marie", "--grant", "-abc", "---"]) {
      expect(isHearthUsername(name), name).toBe(false);
    }
  });

  it("refuse tout ce que le shell, l'oeil ou la longueur peuvent détourner", () => {
    for (const name of HOSTILE) {
      expect(isHearthUsername(name), JSON.stringify(name.slice(0, 40))).toBe(false);
    }
  });

  it("ne tolère pas le retour à la ligne final que `$` d'une regex laisserait passer", () => {
    expect(isHearthUsername("hearth\n")).toBe(false);
    expect(isSystemAccountName("hearth\n")).toBe(false);
  });
});
