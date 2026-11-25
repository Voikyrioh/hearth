import { safeText } from "@/audit/text";
import { type MessageKey, t } from "@/i18n";
import type { ModItem, ModOrigin, ModsGame, ModsView, VerdictCode } from "@/link";
import { canAct, type RightsGate } from "@/services/rights";

/**
 * Les mods d'un serveur Minecraft (HRT-261, `BR-MOD-020` à `033`) : des fonctions pures, sans E/S. Le jugement
 * (compatibilité, doublons, dépendances) est celui de l'agent, qui envoie des CODES ; l'interface les met en phrases. Un
 * nom de fichier ou de mod est un texte inoffensif (`safeText`), jamais du HTML. Un refus de compatibilité dit seulement
 * que le mod n'est pas compatible, jamais une plage de versions (décision de Voiky du 2026-11-23).
 */

const LOADER_NAMES: Record<string, string> = {
  fabric: "Fabric",
  quilt: "Quilt",
  forge: "Forge",
  neoforge: "NeoForge",
};

/** Le nom d'un moteur : un moteur que ce client ne connaît pas est montré tel quel, rendu inoffensif. */
export function loaderName(loader: string | null): string {
  if (loader === null) return t("mods.loaderUnknown");
  return LOADER_NAMES[loader] ?? safeText(loader);
}

/** Les gestes d'un lot sur un fichier existant (BR-MOD-024, 032). */
export type Gesture = "retire" | "restore" | "disable" | "enable";

/** Le nom montré d'un mod : son nom, sinon son identifiant, sinon son fichier, sinon son rang. */
export function modName(
  item: Pick<ModItem, "identity" | "file" | "rank" | "readableName">,
): string {
  const named = item.identity?.name ?? item.identity?.id ?? (item.readableName ? item.file : null);
  if (named) return safeText(named);
  return t("mods.unreadableName", { rank: item.rank + 1 });
}

/** Le nom du fichier tel que la liste le montre, ou rien quand le nom est hors de l'alphabet sûr. */
export function fileLabel(item: Pick<ModItem, "file" | "readableName">): string | null {
  return item.readableName && item.file ? safeText(item.file) : null;
}

const ORIGIN_KEYS: Record<ModOrigin, MessageKey> = {
  server_pack: "mods.origin.serverPack",
  hearth: "mods.origin.hearth",
  modrinth_pack: "mods.origin.modrinthPack",
  manual: "mods.origin.manual",
  unknown: "mods.origin.unknown",
};

export function originLabel(origin: ModOrigin): string {
  return t(ORIGIN_KEYS[origin]);
}

/** Les quatre origines, dans l'ordre de la légende sous le tableau (BR-MOD-033). */
export const ORIGIN_LEGEND: { origin: ModOrigin; term: MessageKey; text: MessageKey }[] = [
  { origin: "server_pack", term: "mods.origin.serverPack", text: "mods.legend.serverPack" },
  { origin: "hearth", term: "mods.origin.hearth", text: "mods.legend.hearth" },
  { origin: "modrinth_pack", term: "mods.origin.modrinthPack", text: "mods.legend.modrinthPack" },
  { origin: "manual", term: "mods.origin.manual", text: "mods.legend.manual" },
];

export type PillTone = "ok" | "warn" | "crit" | "neutral";
export interface Pill {
  tone: PillTone;
  label: string;
}

const VERDICT_PILLS: Record<VerdictCode, { tone: PillTone; key: MessageKey }> = {
  compatible: { tone: "ok", key: "mods.verdict.compatible" },
  unverified: { tone: "warn", key: "mods.verdict.unverified" },
  to_complete: { tone: "warn", key: "mods.verdict.toComplete" },
  incompatible_loader: { tone: "crit", key: "mods.verdict.incompatible" },
  incompatible_minecraft: { tone: "crit", key: "mods.verdict.incompatible" },
  duplicate: { tone: "warn", key: "mods.verdict.duplicate" },
  unrecognized: { tone: "neutral", key: "mods.verdict.unrecognized" },
  unknown: { tone: "warn", key: "mods.verdict.unverified" },
};

/** La pastille d'état d'une ligne : « Retiré », « Désactivé », ou le verdict (BR-MOD-021, 032). */
export function statePill(item: ModItem): Pill {
  if (item.place === "retired") return { tone: "neutral", label: t("mods.state.retired") };
  if (!item.enabled) return { tone: "neutral", label: t("mods.state.disabled") };
  const pill = VERDICT_PILLS[item.verdict.code];
  return { tone: pill.tone, label: t(pill.key) };
}

/** Le verdict d'un mod est un problème QUI EMPÊCHE le serveur de démarrer s'il est activé (bandeau, BR-MOD-021). */
export function isBlocking(item: ModItem): boolean {
  if (item.place !== "active" || !item.enabled) return false;
  const code = item.verdict.code;
  return (
    code === "incompatible_loader" ||
    code === "incompatible_minecraft" ||
    code === "duplicate" ||
    code === "to_complete"
  );
}

/** Les noms des mods dont l'identifiant est dans la liste (l'identifiant sinon), rendus inoffensifs. */
function namesOf(ids: readonly string[], view: ModsView | null): string[] {
  return ids.map((id) => {
    const found = view?.mods.find((mod) => mod.identity?.id === id);
    if (found?.identity?.name) return safeText(found.identity.name);
    // Un mod absent n'a pas de nom lisible : son identifiant, avec une majuscule (« Balm », pas « balm »).
    const shown = safeText(id);
    return shown.charAt(0).toUpperCase() + shown.slice(1);
  });
}

function joined(names: readonly string[]): string {
  if (names.length <= 1) return names[0] ?? "";
  return `${names.slice(0, -1).join(", ")} ${t("mods.and")} ${names[names.length - 1]}`;
}

/** La phrase du verdict d'une ligne, ou rien quand le mod est compatible (BR-MOD-021). */
export function verdictSentence(
  item: ModItem,
  game: ModsGame,
  view: ModsView | null,
): string | null {
  switch (item.verdict.code) {
    case "compatible":
      return null;
    case "unverified":
    case "unknown":
      return game.minecraft === null
        ? t("mods.sentence.unverifiedServer")
        : t("mods.sentence.unverified");
    case "to_complete": {
      const names = namesOf(item.verdict.missing, view);
      return names.length === 1
        ? t("mods.sentence.toCompleteOne", { name: names[0] ?? "" })
        : t("mods.sentence.toComplete", { names: joined(names) });
    }
    case "incompatible_loader":
      return t("mods.sentence.incompatibleLoader", { loader: loaderName(game.loader) });
    case "incompatible_minecraft":
      return t("mods.sentence.incompatibleMinecraft", { minecraft: game.minecraft ?? "?" });
    case "duplicate":
      return t("mods.sentence.duplicate", { name: modName(item) });
    case "unrecognized":
      return t("mods.sentence.unrecognized");
  }
}

/** Le bandeau d'un mod qui empêche le serveur de démarrer (maquette Main, BR-MOD-021). */
export function blockingBanner(
  item: ModItem,
  game: ModsGame,
  view: ModsView | null = null,
): string {
  const name = modName(item);
  switch (item.verdict.code) {
    case "duplicate":
      return t("mods.banner.duplicate", { name });
    case "incompatible_loader":
      return t("mods.banner.incompatibleLoader", { name, loader: loaderName(game.loader) });
    case "to_complete": {
      const names = namesOf(item.verdict.missing, view);
      return names.length === 1
        ? t("mods.banner.toCompleteOne", { name, needed: names[0] ?? "" })
        : t("mods.banner.toComplete", { name, needed: joined(names) });
    }
    case "incompatible_minecraft":
      return t("mods.banner.incompatibleMinecraft", { name, minecraft: game.minecraft ?? "?" });
    default:
      // Un autre verdict n'empêche pas le démarrage (`isBlocking`) : ce texte ne dit rien de faux sur la cause.
      return t("mods.banner.blocking", { name });
  }
}

/**
 * Les mods qui empêcheront le démarrage APRÈS le lot (rappelés dans « Vérifiez avant d'appliquer ») : un mod que le lot
 * désactive ou retire ne compte plus, un ajout qui ne peut pas démarrer compte.
 */
export function blockersAfter(
  view: ModsView,
  pending: ReadonlyMap<string, Gesture>,
  added: readonly ModItem[],
): string[] {
  const kept = view.mods.filter((mod) => {
    const gesture = mod.file === null ? undefined : pending.get(mod.file);
    if (gesture === "disable" || gesture === "retire") return false;
    if (mod.place === "retired") return gesture === "restore" && mod.verdict.code !== "compatible";
    if (!mod.enabled) return gesture === "enable";
    return true;
  });
  return [...kept.map((mod) => ({ ...mod, place: "active" as const, enabled: true })), ...added]
    .filter(isBlocking)
    .map((mod) => blockingBanner(mod, view.game, view));
}

/** Les mods qui ne peuvent pas démarrer, à dire en bandeau : trois au plus, le reste se compte. */
export function bannerItems(view: ModsView): { items: ModItem[]; more: number } {
  const all = view.mods.filter(isBlocking);
  return { items: all.slice(0, 3), more: Math.max(0, all.length - 3) };
}

// ── L'en-tête et les phrases de l'onglet ──────────────────────────────────────────────────────────

export interface Counts {
  installed: number;
  enabled: number;
  compatible: number;
  incompatible: number;
  /** Activés, mais il leur manque un autre mod : le serveur ne démarrera pas. */
  toComplete: number;
  /** Activés, que Hearth ne peut pas juger (version non vérifiée, fiche illisible). */
  unverified: number;
  disabled: number;
  retired: number;
}

export function countsOf(view: ModsView): Counts {
  const active = view.mods.filter((mod) => mod.place === "active");
  const enabled = active.filter((mod) => mod.enabled);
  return {
    installed: active.length,
    enabled: enabled.length,
    compatible: enabled.filter((mod) => mod.verdict.code === "compatible").length,
    incompatible: enabled.filter((mod) => isBlocking(mod) && mod.verdict.code !== "to_complete")
      .length,
    toComplete: enabled.filter((mod) => mod.verdict.code === "to_complete").length,
    unverified: enabled.filter((mod) =>
      ["unverified", "unknown", "unrecognized"].includes(mod.verdict.code),
    ).length,
    disabled: active.length - enabled.length,
    retired: view.mods.filter((mod) => mod.place === "retired").length,
  };
}

function plural(
  n: number,
  one: MessageKey,
  many: MessageKey,
  extra: Record<string, string | number> = {},
): string {
  return t(n === 1 ? one : many, { n, ...extra });
}

export function headingOf(view: ModsView): string {
  const counts = countsOf(view);
  if (counts.installed === 0) return t("mods.headingNone");
  return t(counts.installed === 1 ? "mods.headingOne" : "mods.heading", {
    installed: counts.installed,
    enabled: counts.enabled,
  });
}

export function subheadingOf(view: ModsView): string {
  const counts = countsOf(view);
  if (counts.installed === 0) return t("mods.subheadingNone");
  const parts = [
    plural(counts.compatible, "mods.sub.compatibleOne", "mods.sub.compatible", {
      loader: loaderName(view.game.loader),
      minecraft: view.game.minecraft ?? "?",
    }),
  ];
  if (counts.incompatible > 0)
    parts.push(plural(counts.incompatible, "mods.sub.incompatibleOne", "mods.sub.incompatible"));
  if (counts.toComplete > 0)
    parts.push(plural(counts.toComplete, "mods.sub.toCompleteOne", "mods.sub.toComplete"));
  if (counts.unverified > 0)
    parts.push(plural(counts.unverified, "mods.sub.unverifiedOne", "mods.sub.unverified"));
  if (counts.disabled > 0)
    parts.push(plural(counts.disabled, "mods.sub.disabledOne", "mods.sub.disabled"));
  return `${parts.join(", ")}. ${t("mods.sub.restart")}`;
}

const SOURCE_KEYS: Record<string, MessageKey> = {
  setting: "mods.versionSource.setting",
  log: "mods.versionSource.log",
  modrinth_manifest: "mods.versionSource.modrinth",
};

/** La version de Minecraft du serveur et d'où elle vient (BR-MOD-021) ; jamais devinée. */
export function versionLine(game: ModsGame): string {
  if (game.minecraft === null) return t("mods.versionUnknown");
  const source = SOURCE_KEYS[game.minecraftFrom];
  return source
    ? t("mods.versionLine", { minecraft: game.minecraft, source: t(source) })
    : t("mods.versionLineBare", { minecraft: game.minecraft });
}

/** Les phrases de l'onglet pour un serveur à mods : mise à jour à chaque démarrage, pack Modrinth (BR-MOD-028). */
export function notesOf(game: ModsGame): string[] {
  const notes: string[] = [];
  if (game.updatesEveryStart) notes.push(t("mods.note.latest"));
  if (game.pack === "modrinth_pack") notes.push(t("mods.note.modrinth"));
  return notes;
}

/** Les moteurs sans mods (BR-MOD-030) : le titre, la phrase, et si « Ajouter un serveur de jeu » est proposé. */
export function noModsText(
  applicable: ModsView["applicable"],
): { title: string; text: string; offer: boolean } | null {
  switch (applicable) {
    case "mods":
      return null;
    case "plugins":
      return { title: t("mods.plugins.title"), text: t("mods.plugins.text"), offer: false };
    case "no_mods":
      return { title: t("mods.noMods.title"), text: t("mods.noMods.text"), offer: true };
    default:
      return { title: t("mods.noWay.title"), text: t("mods.noWay.text"), offer: false };
  }
}

// ── Ce que chaque ligne permet ───────────────────────────────────────────────────────────────────

export interface GestureGate {
  allowed: boolean;
  /** Pourquoi le geste est inactif ; absent quand il est permis, ou que le droit est dit ailleurs. */
  reason: string | null;
  /** Le droit manque : la raison est celle que le détail écrit déjà (`ServiceRightReason`). */
  rights: boolean;
}

/** Ce qu'une ligne permet : le droit, le pack Modrinth (BR-MOD-024, 032), un nom illisible (BR-MOD-020). */
export function gestureGate(item: ModItem, gate: RightsGate): GestureGate {
  if (!canAct(gate)) return { allowed: false, reason: null, rights: true };
  if (!item.readableName || item.file === null) {
    return { allowed: false, reason: t("mods.reason.unreadableName"), rights: false };
  }
  if (item.origin === "modrinth_pack") {
    return { allowed: false, reason: t("mods.reason.modrinth"), rights: false };
  }
  return { allowed: true, reason: null, rights: false };
}

const PENDING_KEYS: Record<Gesture, MessageKey> = {
  disable: "mods.pending.disable",
  enable: "mods.pending.enable",
  retire: "mods.pending.retire",
  restore: "mods.pending.restore",
};

/** La ligne de l'en-tête d'un serveur Minecraft : « Minecraft Java, Fabric, version 1.20.1 » (ce que l'on sait de lui). */
export function engineLine(game: ModsGame): string {
  const parts = [t("mods.engine.java")];
  if (game.loader !== null) parts.push(loaderName(game.loader));
  if (game.minecraft !== null) parts.push(t("mods.engine.version", { minecraft: game.minecraft }));
  return parts.join(", ");
}

export function pendingText(gesture: Gesture): string {
  return t(PENDING_KEYS[gesture]);
}

/** Retirer ou désactiver un mod dont d'autres ont besoin : dit avant, sans bloquer (BR-MOD-024, 032). */
export function dependentsNote(
  item: ModItem,
  view: ModsView,
  pending: ReadonlyMap<string, Gesture>,
): string | null {
  const needing = item.verdict.neededBy.filter((id) => {
    const user = view.mods.find(
      (mod) => mod.identity?.id === id && mod.place === "active" && mod.enabled,
    );
    const gesture = user?.file ? pending.get(user.file) : undefined;
    return user !== undefined && gesture !== "disable" && gesture !== "retire";
  });
  if (needing.length === 0) return null;
  const names = namesOf(needing, view);
  return names.length === 1
    ? t("mods.dependents.one", { name: names[0] ?? "" })
    : t("mods.dependents.many", { names: joined(names) });
}

/** Réactiver un mod alors qu'un autre fichier du même identifiant est actif : refusé avant l'envoi (BR-MOD-032). */
export function enableConflict(
  item: ModItem,
  view: ModsView,
  pending: ReadonlyMap<string, Gesture>,
): string | null {
  const id = item.identity?.id;
  if (!id) return null;
  const other = view.mods.find(
    (mod) =>
      mod !== item &&
      mod.place === "active" &&
      mod.enabled &&
      mod.identity?.id === id &&
      (mod.file === null || pending.get(mod.file) !== "disable"),
  );
  return other ? t("mods.reason.alreadyActive") : null;
}
