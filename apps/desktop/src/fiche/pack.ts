import { safeText } from "@/audit/text";
import { type MessageKey, t } from "@/i18n";
import type { FicheDetail, FichePackGenre, PackCheck, PackInspect, PackRefusal } from "@/link";
import { formatSize } from "@/serviceOps/messages";

/**
 * Le pack de mods de l'installation d'un serveur de jeu (HRT-124, HRT-125, BR-MOD-006 à 009, 016) : des fonctions pures
 * sans E/S. Le genre d'un pack est LU PAR L'AGENT dans le contenu de l'archive : ici, jamais un champ à remplir. La
 * fiche dit quels genres elle prend et avec quels moteurs de serveur (variantes) ; l'agent est l'arbitre au lancement.
 */

/** Le pack déposé que la saisie garde : l'identifiant de l'agent, le nom choisi (pour l'écran) et ce que l'agent en a lu. */
export interface PackDraft {
  deposit: string;
  /** Le nom du fichier choisi, tel que l'utilisateur le connaît ; jamais envoyé à l'agent. */
  fileName: string;
  check: PackCheck | null;
}

/** La voie « pack déposé » de la fiche : ses genres, ou `null` quand la fiche n'offre pas le dépôt d'un pack. */
export function packGenres(detail: FicheDetail): FichePackGenre[] | null {
  const mods = detail.mods;
  if (mods === null || !mods.ways.includes("drop_pack") || mods.packGenres.length === 0)
    return null;
  return mods.packGenres;
}

/** Les moteurs de serveur (variantes) que la fiche permet avec le genre lu. */
export function allowedVariants(detail: FicheDetail, pack: PackInspect): string[] {
  return packGenres(detail)?.find((genre) => genre.kind === pack.kind)?.variants ?? [];
}

/** Le pack lu, ou `null` tant que l'agent n'a rien lu (ou l'a refusé). */
export function readPack(draft: PackDraft | null): PackInspect | null {
  return draft?.check?.kind === "read" ? draft.check.pack : null;
}

export type PackIssue =
  /** La case est cochée mais aucun fichier n'est choisi. */
  | "no_file"
  /** L'agent n'a pas encore dit ce qu'est ce pack. */
  | "unread"
  /** L'agent a refusé le pack : il n'est pas installable. */
  | "refused"
  /** La fiche ne prend pas ce genre de pack. */
  | "genre_not_offered"
  /** Le moteur choisi ne convient pas à ce genre de pack. */
  | "engine";

/** Ce qui empêche de continuer avec ce pack, ou `null`. Sans la case « pack de mods » cochée, aucun défaut. */
export function packIssue(
  detail: FicheDetail,
  wanted: boolean,
  draft: PackDraft | null,
  variant: string | null,
): PackIssue | null {
  if (!wanted) return null;
  if (draft === null) return "no_file";
  if (draft.check === null) return "unread";
  if (draft.check.kind === "refused") return "refused";
  const allowed = allowedVariants(detail, draft.check.pack);
  if (allowed.length === 0) return "genre_not_offered";
  return variant !== null && allowed.includes(variant) ? null : "engine";
}

export function genreLabel(kind: PackInspect["kind"]): string {
  if (kind === "server_pack") return t("pack.genreServer");
  if (kind === "modrinth_pack") return t("pack.genreModrinth");
  return t("pack.genreUnknown");
}

const LOADERS: Record<string, string> = {
  "fabric-loader": "Fabric",
  "quilt-loader": "Quilt",
  forge: "Forge",
  neoforge: "NeoForge",
};

/** Ce que l'agent a lu, en phrases courtes pour l'écran (nombres de l'agent, textes nettoyés par lui puis par `safeText`). */
export function packFacts(pack: PackInspect): string[] {
  const facts = [
    t("pack.factFiles", { n: pack.files }),
    t("pack.factSize", {
      size: formatSize(pack.sizeBytes),
      unpacked: formatSize(pack.unpackedBytes),
    }),
  ];
  if (pack.kind === "server_pack") {
    facts.unshift(t("pack.factMods", { n: pack.mods }));
    if (pack.topDirs.length > 0) {
      facts.push(t("pack.factDirs", { dirs: pack.topDirs.map((dir) => safeText(dir)).join(", ") }));
    }
  }
  const modrinth = pack.modrinth;
  if (modrinth !== null) {
    facts.unshift(
      t("pack.factModrinth", {
        name: safeText(modrinth.name),
        version: safeText(modrinth.version),
      }),
    );
    const loader = modrinth.loader === null ? null : (LOADERS[modrinth.loader] ?? null);
    const announced = [
      loader === null
        ? null
        : `${loader}${modrinth.loaderVersion ? ` ${safeText(modrinth.loaderVersion)}` : ""}`,
      modrinth.minecraft ? `Minecraft ${safeText(modrinth.minecraft)}` : null,
    ].filter((part): part is string => part !== null);
    if (announced.length > 0) facts.push(t("pack.factAnnounces", { what: announced.join(", ") }));
  }
  return facts;
}

/** Les phrases de « Vérifiez avant de lancer » (BR-MOD-009) : celle qui concerne le genre, puis, toujours, la dernière. */
export function packSentences(pack: PackInspect): string[] {
  const sentences: string[] = [];
  if (pack.kind === "modrinth_pack" && pack.modrinth !== null) {
    sentences.push(
      t("pack.checkModrinth", {
        n: pack.modrinth.downloads,
        hosts: pack.modrinth.hosts.map((host) => safeText(host)).join(", "),
      }),
    );
  } else if (pack.kind === "server_pack") {
    sentences.push(t("pack.checkServer", { n: pack.mods }));
  }
  sentences.push(t("pack.checkAlways"));
  return sentences;
}

const REASONS: Record<string, MessageKey> = {
  ARCHIVE_REFUSED: "pack.refusedArchive",
  PACK_PATH_ESCAPES: "pack.refusedPath",
  PACK_INDEX_INVALID: "pack.refusedIndex",
  PACK_WRAPPED_IN_FOLDER: "pack.refusedWrapped",
  PACK_UNRECOGNIZED: "pack.refusedUnrecognized",
};

/**
 * Le refus d'un pack en phrases (BR-MOD-008, BR-MOD-016) : jamais un texte de l'agent, jamais un nom d'entrée. Une liste de
 * téléchargements CurseForge dit trois choses : ce que c'est, quoi télécharger à la place, l'autre voie par Modrinth.
 */
export function packRefusalSentences(refusal: PackRefusal): string[] {
  if (refusal.kind === "unknown") return [t("pack.refusedUnknown")];
  if (refusal.reason === "PACK_IS_A_DOWNLOAD_LIST") {
    return [
      refusal.mods === null ? t("pack.listWhatNoCount") : t("pack.listWhat", { n: refusal.mods }),
      t("pack.listInstead"),
      t("pack.listModrinth"),
    ];
  }
  if (refusal.reason === "PACK_HOST_FORBIDDEN") {
    return [
      refusal.host === null
        ? t("pack.refusedHostUnknown")
        : t("pack.refusedHost", { host: safeText(refusal.host) }),
    ];
  }
  const key = REASONS[refusal.reason];
  return [key === undefined ? t("pack.refusedOther") : t(key)];
}

/** La phrase d'un défaut qui empêche de continuer (`packIssue`). */
export function packIssueText(issue: PackIssue): string {
  switch (issue) {
    case "no_file":
      return t("pack.issueNoFile");
    case "unread":
      return t("pack.issueUnread");
    case "refused":
      return t("pack.issueRefused");
    case "genre_not_offered":
      return t("pack.issueGenre");
    case "engine":
      return t("pack.issueEngine");
  }
}
