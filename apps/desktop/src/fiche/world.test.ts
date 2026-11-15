import { describe, expect, it } from "vitest";
import {
  ficheKnowsWorlds,
  hiddenByWorld,
  installRefusalText,
  newWorldDraft,
  notCopiedLines,
  notEnoughRoom,
  refusalText,
  worldChoice,
  worldMissing,
  worldPickOf,
} from "./world";

// Les règles pures de la reprise d'un monde (HRT-106, BR-MONDE-*) : aucune E/S.
const detail = { worldImport: { namesSetting: "world_name", carriedByWorld: ["size", "seed"] } };

describe("ce que le monde repris porte (BR-MONDE-014)", () => {
  it("cache le nom du monde et les réglages que le monde décide, seulement pendant une reprise", () => {
    expect([...hiddenByWorld(detail, { resume: true })].sort()).toEqual([
      "seed",
      "size",
      "world_name",
    ]);
    expect(hiddenByWorld(detail, { resume: false }).size).toBe(0);
    expect(hiddenByWorld({ worldImport: null }, { resume: true }).size).toBe(0);
  });

  it("ne connaît pas la reprise d'un monde quand la fiche ne dit pas ce qu'est un monde", () => {
    expect(ficheKnowsWorlds(detail)).toBe(true);
    expect(ficheKnowsWorlds({ worldImport: null })).toBe(false);
  });
});

describe("le monde choisi", () => {
  it("n'est une demande qu'une fois choisi, avec ses deux gestes", () => {
    const world = newWorldDraft();
    expect(worldMissing(world)).toBe(false);
    expect(worldPickOf(world, true)).toBeNull();
    world.resume = true;
    expect(worldMissing(world)).toBe(true);
    world.folder = "/srv/jeux";
    world.path = "Worlds/Alpha.wld";
    expect(worldMissing(world)).toBe(false);
    expect(worldPickOf(world, false)).toEqual({
      folder: "/srv/jeux",
      path: "Worlds/Alpha.wld",
      versionUnderstood: false,
      keepIfChanged: false,
    });
  });
});

describe("la liste et la place (BR-MONDE-002, 003, 012)", () => {
  it("dit « aucun dossier ouvert » quand le serveur n'en a aucun", () => {
    expect(worldChoice({ roots: [], maxDepth: 3, maxEntries: 5000 })).toEqual({
      kind: "no_folder",
    });
  });

  it("dit la recherche tronquée et les dossiers illisibles", () => {
    const view = {
      roots: [
        { folder: "/a", readable: true, truncated: true, worlds: [] },
        { folder: "/b", readable: false, truncated: false, worlds: [] },
      ],
      maxDepth: 3,
      maxEntries: 5000,
    };
    expect(worldChoice(view)).toEqual({ kind: "folders", truncated: true, unreadable: 1 });
  });

  it("manque de place seulement quand le disque est connu et plus petit que la copie", () => {
    expect(notEnoughRoom({ bytes: 10, freeBytes: 5 })).toBe(true);
    expect(notEnoughRoom({ bytes: 10, freeBytes: 10 })).toBe(false);
    expect(notEnoughRoom({ bytes: 10, freeBytes: null })).toBe(false);
  });
});

describe("les textes", () => {
  it("cite ce qui n'est pas copié, jamais un nom brut du serveur", () => {
    const lines = notCopiedLines({
      files: 1,
      bytes: 1,
      notCopied: 3,
      notCopiedNames: ["a.bak", "b.bak"],
      skippedLinks: 1,
      skippedLinkNames: ["lien"],
      partial: false,
      freeBytes: null,
    });
    expect(lines).toHaveLength(2);
    expect(lines[0]).toContain("a.bak, b.bak et 1 autres");
    expect(lines[1]).toContain("lien");
  });

  it("dit un refus d'un monde par son code, et un code inconnu par le refus général", () => {
    expect(refusalText("not_a_world")).toBe("Ce n'est pas un monde que ce jeu reconnaît.");
    expect(refusalText("from_the_future" as never)).toBe("Ce monde ne peut pas être repris.");
  });

  it("ne dit que les refus d'installation qui visent le monde", () => {
    expect(
      installRefusalText({ kind: "invalid", field: "world", reason: "version_not_understood" }),
    ).toContain("risque lié à la version");
    expect(installRefusalText({ kind: "heavy_io_busy" })).toContain("Attendez sa fin");
    expect(installRefusalText({ kind: "invalid", field: "name", reason: "bad_format" })).toBeNull();
    expect(installRefusalText({ kind: "deploy_limit" })).toBeNull();
  });
});
