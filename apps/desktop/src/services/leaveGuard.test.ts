import { describe, expect, it } from "vitest";
import { holdsNavigation } from "./leaveGuard";

const here = { name: "service", query: { service: "a" } };
const elsewhere = { name: "servers", query: {} };
const context = { dirty: true, confirming: false, serverGone: false };

describe("la garde de sortie de la fiche d'un service", () => {
  it("retient une sortie volontaire quand un lot est en préparation", () => {
    expect(holdsNavigation(context, elsewhere, here)).toBe(true);
  });

  it("ne retient rien sans lot, ni pendant une confirmation, ni sur le même service", () => {
    expect(holdsNavigation({ ...context, dirty: false }, elsewhere, here)).toBe(false);
    expect(holdsNavigation({ ...context, confirming: true }, elsewhere, here)).toBe(false);
    expect(holdsNavigation(context, { ...here }, here)).toBe(false);
  });

  it("laisse passer une redirection", () => {
    const redirected = { ...elsewhere, redirectedFrom: {} };
    expect(holdsNavigation(context, redirected, here)).toBe(false);
  });

  it("laisse passer quand le serveur n'existe plus", () => {
    expect(holdsNavigation({ ...context, serverGone: true }, elsewhere, here)).toBe(false);
  });
});
