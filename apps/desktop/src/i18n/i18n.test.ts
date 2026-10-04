import { describe, expect, it } from "vitest";
import { fr } from "./fr";
import { errorKey, t } from "./index";

function leaves(node: unknown, path = ""): [string, string][] {
  if (typeof node === "string") return [[path, node]];
  return Object.entries(node as Record<string, unknown>).flatMap(([key, value]) =>
    leaves(value, path ? `${path}.${key}` : key),
  );
}

describe("i18n", () => {
  it("resolves a key to its French text", () => {
    expect(t("welcome.title")).toBe("Bienvenue dans Hearth");
    expect(t("settings.launchAtStartup")).toBe("Lancer Hearth au démarrage de Windows");
  });

  it("never uses an em dash (copy rule)", () => {
    for (const [path, text] of leaves(fr)) {
      expect(text, path).not.toContain("—");
    }
  });

  it("has no empty text", () => {
    for (const [path, text] of leaves(fr)) {
      expect(text.trim(), path).not.toBe("");
    }
  });

  it("maps every typed Rust error kind to a French text", () => {
    expect(t(errorKey("store"))).toBe("Impossible de lire ou d'enregistrer tes réglages.");
    expect(t(errorKey("autostart"))).toContain("Windows");
    expect(t(errorKey("logs"))).toContain("journaux");
  });
});
