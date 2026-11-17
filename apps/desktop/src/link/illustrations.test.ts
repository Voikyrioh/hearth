import { describe, expect, it } from "vitest";
import {
  agentIllustrationUrl,
  illustrationBase,
  isSteamAppId,
  steamHeaderUrl,
} from "./illustrations";

const HEX = "ab".repeat(32);

describe("les adresses des images de jeux (ADR-0061)", () => {
  it("écrit le protocole selon le système", () => {
    expect(illustrationBase(false)).toBe("hearth-illustration://localhost");
    expect(illustrationBase(true)).toBe("http://hearth-illustration.localhost");
  });

  it("construit l'adresse Steam depuis un entier valide seulement", () => {
    const base = illustrationBase(false);
    expect(steamHeaderUrl(base, 105600)).toBe(
      "hearth-illustration://localhost/steam/105600/header",
    );
    for (const bad of [0, -1, 1.5, 4_294_967_296, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(steamHeaderUrl(base, bad), String(bad)).toBe("");
    }
    expect(isSteamAppId("105600")).toBe(false);
    expect(isSteamAppId(4_294_967_295)).toBe(true);
  });

  it("construit l'adresse de l'agent, empreinte avec ou sans préfixe, et refuse tout le reste", () => {
    const base = illustrationBase(true);
    const expected = `http://hearth-illustration.localhost/agent/forge/minecraft/${HEX}`;
    expect(agentIllustrationUrl(base, "forge", "minecraft", `sha256:${HEX}`)).toBe(expected);
    expect(agentIllustrationUrl(base, "forge", "minecraft", HEX)).toBe(expected);
    expect(agentIllustrationUrl(base, "forge", "minecraft", "AB".repeat(32))).toBe("");
    expect(agentIllustrationUrl(base, "forge", "minecraft", "ab")).toBe("");
    expect(agentIllustrationUrl(base, "forge", "../x", HEX)).toBe("");
    expect(agentIllustrationUrl(base, "a/b", "minecraft", HEX)).toBe("");
    expect(agentIllustrationUrl(base, "", "minecraft", HEX)).toBe("");
  });
});
