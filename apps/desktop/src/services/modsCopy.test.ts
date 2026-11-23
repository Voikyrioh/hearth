import { describe, expect, it } from "vitest";
import { audienceOf, copyText, entryOf, toInstall } from "./modsCopy";

// La liste à donner aux joueurs (HRT-264, BR-MOD-029) : jamais « Obligatoire », un mod qui ne dit rien est classé chez les joueurs.
const id = (name: string, environment: "both" | "server" | "client" | "unknown") =>
  entryOf({
    id: name.toLowerCase(),
    name,
    version: "1.0",
    loader: "fabric",
    minecraftDeclared: true,
    environment,
  });

describe("la liste des joueurs", () => {
  it("un mod qui dit « serveur » est « Serveur seulement », tous les autres vont chez les joueurs", () => {
    expect(audienceOf("server")).toBe("server");
    for (const environment of ["both", "client", "unknown"] as const) {
      expect(audienceOf(environment)).toBe("players");
    }
  });

  it("copie nom, version, moteur et version de Minecraft, l'adresse en tête, sans les mods du serveur seulement", () => {
    const text = copyText({
      address: "jeu.example.org:25565",
      minecraft: "1.20.1",
      entries: [id("Lithium", "both"), id("Carpet", "server"), id("Mystère", "unknown")],
    });
    expect(text.split("\n")).toEqual([
      "Adresse du serveur : jeu.example.org:25565",
      "Lithium 1.0 (Fabric, Minecraft 1.20.1)",
      "Mystère 1.0 (Fabric, Minecraft 1.20.1)",
    ]);
    expect(text).not.toContain("Obligatoire");
  });

  it("sans adresse connue, la liste commence par son titre ou ses mods ; une version inconnue n'est pas inventée", () => {
    const text = copyText({
      title: "Mods de la sauvegarde du 17 nov. 20:00",
      address: null,
      minecraft: null,
      entries: [{ name: "X", version: null, loader: "quilt", environment: "both" }],
    });
    expect(text.split("\n")).toEqual([
      "Mods de la sauvegarde du 17 nov. 20:00",
      "X ? (Quilt, Minecraft ?)",
    ]);
    expect(toInstall([id("A", "server")])).toEqual([]);
  });
});
