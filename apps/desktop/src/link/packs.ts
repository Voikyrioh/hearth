import type { ModrinthPackDto, PackCheckOutcome, PackInspectDto } from "@/bindings";

/**
 * Un pack de mods déposé (HRT-124, ADR-0054, BR-MOD-007, BR-MOD-008, BR-MOD-016), côté pont. Le genre est LU PAR L'AGENT
 * dans le contenu de l'archive : l'interface ne le demande jamais et ne le devine jamais. Un refus est un CODE fixe de
 * l'agent plus, selon lui, un nombre ou un nom d'hôte : jamais un nom d'entrée de l'archive, jamais un texte de l'agent.
 *
 * - `inspectPack` est une CONSULTATION (ni acte, ni journal) : rien n'est rangé, le dépôt reste en attente ;
 * - le fichier arrive par les transferts reprenables (HRT-95, HRT-96 : `startDeposit`, `readTransfers`) ; l'identifiant
 *   du transfert terminé est l'identifiant du dépôt que `inspectPack` lit.
 */

/** Le genre d'un pack, tel que l'agent l'a reconnu. `unknown` : un genre plus récent que ce client. */
export type PackKind = "server_pack" | "modrinth_pack" | "unknown";

/** Ce qu'un pack Modrinth dit de lui-même. */
export interface ModrinthPack {
  name: string;
  version: string;
  minecraft: string | null;
  /** `fabric-loader`, `quilt-loader`, `forge` ou `neoforge`. */
  loader: string | null;
  loaderVersion: string | null;
  /** Le nombre de fichiers que le programme du jeu télécharge. */
  downloads: number;
  hosts: string[];
}

/** Ce que l'agent a lu dans le pack. */
export interface PackInspect {
  kind: PackKind;
  sizeBytes: number;
  files: number;
  /** Pack serveur : le nombre de mods ; pack Modrinth : 0. */
  mods: number;
  unpackedBytes: number;
  /** Les dossiers de premier niveau. */
  topDirs: string[];
  modrinth: ModrinthPack | null;
}

/** Pourquoi l'agent ne dit pas ce qu'est ce pack : un code fixe et, selon lui, un nombre ou un hôte. */
export type PackRefusal =
  /** Aucun dépôt complet sous cet identifiant : à déposer de nouveau. */
  | { kind: "unknown" }
  | { kind: "refused"; reason: string; mods: number | null; host: string | null };

/** L'issue de la lecture d'un pack. */
export type PackCheck =
  | { kind: "read"; pack: PackInspect }
  | { kind: "refused"; refusal: PackRefusal };

function toModrinth(dto: ModrinthPackDto): ModrinthPack {
  return {
    name: dto.name,
    version: dto.version,
    minecraft: dto.minecraft,
    loader: dto.loader,
    loaderVersion: dto.loaderVersion,
    downloads: dto.downloads,
    hosts: [...dto.hosts],
  };
}

function toInspect(dto: PackInspectDto): PackInspect {
  return {
    kind: dto.genre,
    // Un nombre illisible (`null` sur le fil) vaut zéro : l'écran ne montre jamais un chiffre inventé.
    sizeBytes: dto.sizeBytes ?? 0,
    files: dto.files ?? 0,
    mods: dto.mods ?? 0,
    unpackedBytes: dto.unpackedBytes ?? 0,
    topDirs: [...dto.topDirs],
    modrinth: dto.modrinth ? toModrinth(dto.modrinth) : null,
  };
}

export function toPackCheck(dto: PackCheckOutcome): PackCheck {
  if (dto.kind === "read") return { kind: "read", pack: toInspect(dto.pack) };
  return dto.refusal.kind === "unknown"
    ? { kind: "refused", refusal: { kind: "unknown" } }
    : {
        kind: "refused",
        refusal: {
          kind: "refused",
          reason: dto.refusal.reason,
          mods: dto.refusal.mods,
          host: dto.refusal.host,
        },
      };
}
