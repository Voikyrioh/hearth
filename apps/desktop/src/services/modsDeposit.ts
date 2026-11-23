import { safeText } from "@/audit/text";
import { type MessageKey, t } from "@/i18n";
import type { ModCheckFile, ModsGame } from "@/link";
import { loaderName } from "@/services/mods";

/**
 * Déposer des mods et lire le contrôle (HRT-262, `BR-MOD-022`, `023`) : des fonctions pures, sans E/S. Le jugement est
 * celui de l'agent (un code par fichier) ; l'interface le met en phrases. Un mod refusé dit seulement qu'il n'est pas
 * compatible, jamais une plage de versions de Minecraft (décision de Voiky du 2026-11-23).
 */

/** Un lot compte 8 fichiers au plus (BR-MOD-022). */
export const MAX_DEPOSITS = 8;

/** Seuls les fichiers `.jar` se déposent. */
export function isJar(path: string): boolean {
  return /\.jar$/i.test(path);
}

/** Le dernier nom d'un chemin, avec ses deux séparateurs, rendu inoffensif. */
export function nameOfPath(path: string): string {
  const parts = path.split(/[\\/]/);
  return safeText(parts[parts.length - 1] ?? path);
}

const REFUSALS: Record<string, MessageKey> = {
  MOD_UNRECOGNIZED: "mods.refusal.unrecognized",
  MOD_ARCHIVE_REFUSED: "mods.refusal.archive",
  MOD_TOO_LARGE: "mods.refusal.tooLarge",
  MOD_DUPLICATE_IN_BATCH: "mods.refusal.duplicate",
  MOD_NAME_TAKEN: "mods.refusal.nameTaken",
  MOD_IN_MODRINTH_PACK: "mods.refusal.modrinth",
  MOD_SAME_VERSION: "mods.refusal.sameVersion",
};

/** La phrase d'un dépôt refusé (BR-MOD-021 à 023) : sans plage de versions. */
export function refusalSentence(file: ModCheckFile, game: ModsGame): string {
  const code = file.refusal ?? "";
  if (code === "MOD_INCOMPATIBLE_LOADER") {
    const mod = file.refusalValues?.loader ?? file.identity?.loader ?? null;
    const server = file.refusalValues?.serverLoader ?? game.loader;
    return t("mods.refusal.loader", { mod: loaderName(mod), server: loaderName(server) });
  }
  if (code === "MOD_INCOMPATIBLE_MINECRAFT") {
    return t("mods.refusal.minecraft", {
      minecraft: file.refusalValues?.serverMinecraft ?? game.minecraft ?? "?",
    });
  }
  const key = REFUSALS[code];
  return key ? t(key) : t("mods.refusal.other");
}

/** La phrase d'un fichier accepté (ajout, remplacement, à compléter, non vérifié). */
export function acceptedSentence(file: ModCheckFile, game: ModsGame): string {
  const verdict = file.verdict?.code ?? "compatible";
  const server = {
    loader: loaderName(game.loader),
    minecraft: game.minecraft ?? "?",
  };
  if (verdict === "to_complete") {
    const missing = file.verdict?.missing ?? [];
    return t("mods.checkSentence.toComplete", {
      names: missing.map((name) => safeText(name)).join(", "),
    });
  }
  if (verdict === "unverified" || game.minecraft === null) {
    return file.outcome === "replace"
      ? t("mods.checkSentence.unverifiedReplace")
      : t("mods.checkSentence.unverified");
  }
  return file.outcome === "replace"
    ? t("mods.checkSentence.replace", server)
    : t("mods.checkSentence.add", server);
}

/** « Contrôle terminé : 2 fichiers prêts, 2 à revoir ». */
export function checkTitle(files: readonly ModCheckFile[]): string {
  const ready = files.filter((file) => file.outcome === "add" || file.outcome === "replace").length;
  const review = files.length - ready;
  const parts: string[] = [];
  if (ready > 0)
    parts.push(t(ready === 1 ? "mods.checkTitle.readyOne" : "mods.checkTitle.ready", { n: ready }));
  if (review > 0)
    parts.push(
      t(review === 1 ? "mods.checkTitle.reviewOne" : "mods.checkTitle.review", { n: review }),
    );
  return t("mods.checkTitle.lead", { parts: parts.join(", ") });
}
