import { safeText } from "@/audit/text";
import { t } from "@/i18n";
import type { ModEnvironment, ModIdentity } from "@/link";
import { loaderName } from "@/services/mods";

/**
 * La liste de mods à donner aux joueurs (HRT-264, HRT-294, `BR-MOD-029`, `035`) : des fonctions pures, UNE seule mise en
 * forme pour l'écran « Ce que vos joueurs doivent installer » et pour « Copier la liste » d'une sauvegarde. Hearth ne
 * sait pas ce qui est obligatoire : il recopie l'indication « environnement » que le mod donne sur lui-même, jamais
 * « Obligatoire ». Un mod qui ne dit rien est « Aussi chez les joueurs » (le plus sûr). Seuls les mods ACTIVÉS comptent.
 */

/** Ce qu'un mod dit de l'endroit où il se charge : chez les joueurs aussi, ou sur le serveur seulement. */
export type Audience = "players" | "server";

export function audienceOf(environment: ModEnvironment): Audience {
  return environment === "server" ? "server" : "players";
}

/** Pourquoi le mod est classé ainsi, en une phrase (BR-MOD-029). */
export function audienceReason(environment: ModEnvironment): string {
  switch (environment) {
    case "server":
      return t("mods.players.reason.server");
    case "both":
      return t("mods.players.reason.both");
    case "client":
      return t("mods.players.reason.client");
    default:
      return t("mods.players.reason.unknown");
  }
}

export interface CopyEntry {
  name: string;
  version: string | null;
  loader: string;
  environment: ModEnvironment;
}

/** Un mod de la liste : son nom, sa version, son moteur et ce qu'il dit de son environnement. */
export function entryOf(identity: ModIdentity): CopyEntry {
  return {
    name: safeText(identity.name ?? identity.id),
    version: identity.version === null ? null : safeText(identity.version),
    loader: identity.loader,
    environment: identity.environment,
  };
}

/** Les mods à installer chez les joueurs : tous sauf ceux qui disent n'agir que sur le serveur. */
export function toInstall(entries: readonly CopyEntry[]): CopyEntry[] {
  return entries.filter((entry) => audienceOf(entry.environment) === "players");
}

/**
 * Le texte copié : l'adresse du serveur en tête quand elle est connue, une ligne de titre éventuelle (« Mods de la
 * sauvegarde du … »), puis une ligne par mod : nom, version, moteur et version de Minecraft.
 */
export function copyText(input: {
  title?: string;
  address: string | null;
  minecraft: string | null;
  entries: readonly CopyEntry[];
}): string {
  const lines: string[] = [];
  if (input.title) lines.push(input.title);
  if (input.address) lines.push(t("mods.players.copyAddress", { address: input.address }));
  for (const entry of toInstall(input.entries)) {
    lines.push(
      t("mods.players.copyLine", {
        name: entry.name,
        version: entry.version ?? "?",
        loader: loaderName(entry.loader),
        minecraft: input.minecraft ?? "?",
      }),
    );
  }
  return lines.join("\n");
}
