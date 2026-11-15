import { describe, expect, it } from "vitest";
import type { FicheDetail, PackInspect } from "@/link";
import { rich } from "@/link/simulated-fiches";
import {
  allowedVariants,
  genreLabel,
  type PackDraft,
  packFacts,
  packGenres,
  packIssue,
  packRefusalSentences,
  packSentences,
} from "./pack";

// Le pack de mods de l'installation (HRT-125, BR-MOD-006 à 009, 016) : des règles pures. Le genre est lu par l'agent ; ici
// seulement ce qu'on en dit, et quel moteur la fiche permet pour lui.

const server: PackInspect = {
  kind: "server_pack",
  sizeBytes: 794_497_024,
  files: 2314,
  mods: 652,
  unpackedBytes: 1_650_000_000,
  topDirs: ["config", "mods"],
  modrinth: null,
};

const modrinth: PackInspect = {
  kind: "modrinth_pack",
  sizeBytes: 1_000_000,
  files: 12,
  mods: 0,
  unpackedBytes: 2_000_000,
  topDirs: ["overrides"],
  modrinth: {
    name: "Mon pack",
    version: "1.2",
    minecraft: "1.20.1",
    loader: "fabric-loader",
    loaderVersion: "0.15.0",
    downloads: 40,
    hosts: ["cdn.modrinth.com", "github.com"],
  },
};

const fiche = (): FicheDetail => rich();

describe("ce que la fiche offre", () => {
  it("n'offre le pack que si elle déclare la voie « pack déposé » et un genre", () => {
    expect(packGenres(fiche())?.map((genre) => genre.kind)).toEqual([
      "server_pack",
      "modrinth_pack",
    ]);
    const none = { ...fiche(), mods: null };
    expect(packGenres(none)).toBeNull();
    const files = {
      ...fiche(),
      mods: { ways: ["drop_files" as const], packGenres: [], filesHint: null },
    };
    expect(packGenres(files)).toBeNull();
  });

  it("dit quels moteurs la fiche permet pour le genre lu", () => {
    expect(allowedVariants(fiche(), server)).toEqual(["modded"]);
    expect(allowedVariants(fiche(), modrinth)).toEqual(["vanilla"]);
    expect(allowedVariants(fiche(), { ...server, kind: "unknown" })).toEqual([]);
  });
});

describe("ce qui empêche de continuer", () => {
  const draftOf = (overrides: Partial<PackDraft> = {}): PackDraft => ({
    path: "C:p.zip",
    deposit: "d1",
    fileName: "p.zip",
    delivered: true,
    check: null,
    ...overrides,
  });
  const read = (pack: PackInspect) => draftOf({ check: { kind: "read" as const, pack } });

  it("n'a aucun défaut sans la case cochée", () => {
    expect(packIssue(fiche(), false, null, "vanilla")).toBeNull();
  });

  it("demande un fichier, puis sa lecture", () => {
    expect(packIssue(fiche(), true, null, "modded")).toBe("no_file");
    expect(packIssue(fiche(), true, draftOf({ deposit: null, delivered: false }), "modded")).toBe(
      "unsent",
    );
    expect(packIssue(fiche(), true, draftOf({ delivered: false }), "modded")).toBe("sending");
    expect(packIssue(fiche(), true, draftOf(), "modded")).toBe("unread");
  });

  it("refuse un pack que l'agent a refusé", () => {
    const refused = draftOf({
      check: { kind: "refused" as const, refusal: { kind: "unknown" as const } },
    });
    expect(packIssue(fiche(), true, refused, "modded")).toBe("refused");
  });

  it("exige le moteur que la fiche permet pour ce genre", () => {
    expect(packIssue(fiche(), true, read(server), "modded")).toBeNull();
    expect(packIssue(fiche(), true, read(server), "vanilla")).toBe("engine");
    expect(packIssue(fiche(), true, read(modrinth), "vanilla")).toBeNull();
    expect(packIssue(fiche(), true, read({ ...server, kind: "unknown" }), "modded")).toBe(
      "genre_not_offered",
    );
  });
});

describe("ce que l'écran dit", () => {
  it("nomme le genre", () => {
    expect(genreLabel("server_pack")).toBe("Pack serveur");
    expect(genreLabel("modrinth_pack")).toBe("Pack Modrinth");
  });

  it("dit les nombres lus par l'agent", () => {
    const facts = packFacts(server);
    expect(facts[0]).toBe("652 mods");
    expect(facts.join(" | ")).toContain("2314 fichiers");
    expect(facts.join(" | ")).toContain("Dossiers : config, mods");
    const mr = packFacts(modrinth).join(" | ");
    expect(mr).toContain("Mon pack, version 1.2");
    expect(mr).toContain("Fabric 0.15.0, Minecraft 1.20.1");
  });

  it("dit avant de lancer ce que le jeu fera, et finit toujours par la phrase des mods", () => {
    const serverSentences = packSentences(server);
    expect(serverSentences[0]).toBe(
      "Ce pack contient 652 mods. Le programme du jeu copie son contenu dans son dossier. Hearth ne l'ouvre pas et ne le vérifie pas.",
    );
    const modrinthSentences = packSentences(modrinth);
    expect(modrinthSentences[0]).toBe(
      "Ce pack fait télécharger 40 fichiers par le programme du jeu, depuis cdn.modrinth.com, github.com. Hearth ne les vérifie pas.",
    );
    for (const list of [serverSentences, modrinthSentences]) {
      expect(list.at(-1)).toBe(
        "Un mod est un programme écrit par quelqu'un d'autre. Il tourne avec les droits du serveur de jeu.",
      );
    }
  });

  it("dit en trois phrases ce qu'est une liste CurseForge et quoi télécharger à la place", () => {
    const sentences = packRefusalSentences({
      kind: "refused",
      reason: "PACK_IS_A_DOWNLOAD_LIST",
      mods: 652,
      host: null,
    });
    expect(sentences).toHaveLength(3);
    expect(sentences[0]).toContain("seulement leur liste (652 mods)");
    expect(sentences[1]).toContain("« Server Pack »");
    expect(sentences[2]).toContain("Modrinth");
  });

  it("dit l'hôte refusé, jamais un nom d'entrée, et un code inconnu par une phrase générale", () => {
    expect(
      packRefusalSentences({
        kind: "refused",
        reason: "PACK_HOST_FORBIDDEN",
        mods: null,
        host: "evil.example",
      }),
    ).toEqual([
      "Ce pack demande de télécharger depuis evil.example, qui n'est pas permis par le format des packs Modrinth.",
    ]);
    expect(
      packRefusalSentences({ kind: "refused", reason: "UN_CODE_FUTUR", mods: null, host: null }),
    ).toEqual(["Hearth ne peut pas installer ce pack."]);
    expect(packRefusalSentences({ kind: "unknown" })[0]).toContain("n'est plus sur le serveur");
  });

  it("n'emploie ni tiret cadratin ni tutoiement", () => {
    const all = [
      ...packFacts(server),
      ...packFacts(modrinth),
      ...packSentences(server),
      ...packSentences(modrinth),
      ...packRefusalSentences({
        kind: "refused",
        reason: "PACK_IS_A_DOWNLOAD_LIST",
        mods: 3,
        host: null,
      }),
    ].join(" ");
    expect(all).not.toContain("—");
    expect(all).not.toMatch(/\b(tu|ton|ta|tes|toi)\b/i);
  });
});
