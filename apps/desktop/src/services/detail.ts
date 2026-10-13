import { isHearthUsername } from "@/accounts/name";
import { type MessageKey, t } from "@/i18n";
import type { RestartMode, ServiceInfo, ServiceMount, ServicePort } from "@/link";

/**
 * Le détail d'un service (HRT-63, BR-SVC-007, 008, 016, 023, 027, 028) : des fonctions PURES, sans E/S. L'agent décide
 * de tout (état, repères, ce qui est réservé) ; ici on ne fait que dire ce qu'il a dit, et toute valeur que ce client
 * ne connaît pas s'affiche de façon neutre (jamais un texte inventé, jamais une exception).
 */

/** Le service ouvert, vu de la liste d'un serveur. */
export type OpenedService =
  /** La liste n'est pas encore arrivée : ni « disparu » ni détail. */
  | { kind: "waiting" }
  | { kind: "found"; service: ServiceInfo }
  /**
   * Le service n'est plus dans la liste (BR-SVC-016). `last` : la dernière version vue (le détail reste lisible,
   * grisé) ou `null` s'il n'a jamais été vu par cet écran ; `sameName` : un AUTRE service de même nom existe, jamais
   * ouvert d'office.
   */
  | { kind: "gone"; last: ServiceInfo | null; sameName: ServiceInfo | null };

/**
 * Retrouve le service ouvert par sa CLÉ STABLE, jamais par son nom (BR-SVC-016). `ready` : la liste est connue
 * (sinon on attend). `last` : la dernière version de ce service que l'écran a vue.
 */
export function openedService(
  services: readonly ServiceInfo[],
  key: string,
  ready: boolean,
  last: ServiceInfo | null,
): OpenedService {
  const found = services.find((service) => service.key === key);
  if (found) return { kind: "found", service: found };
  if (!ready) return { kind: "waiting" };
  const name = last?.name;
  const sameName = name === undefined ? null : (services.find((s) => s.name === name) ?? null);
  return { kind: "gone", last, sameName };
}

/** Le code de sortie et son explication pour les trois cas courants (BR-SVC-023) ; tout autre code est affiché seul. */
export function exitCodeText(code: number): string {
  switch (code) {
    case 0:
      return t("serviceDetail.exitCode", { code, explanation: t("serviceDetail.exit0") });
    case 137:
      return t("serviceDetail.exitCode", { code, explanation: t("serviceDetail.exit137") });
    case 143:
      return t("serviceDetail.exitCode", { code, explanation: t("serviceDetail.exit143") });
    default:
      return t("serviceDetail.exitCodeBare", { code });
  }
}

const RESTART_KEYS: Record<Exclude<RestartMode, "unknown">, MessageKey> = {
  unless_stopped: "serviceDetail.restartUnlessStopped",
  always: "serviceDetail.restartAlways",
  on_failure: "serviceDetail.restartOnFailure",
  none: "serviceDetail.restartNone",
  supervisor: "serviceDetail.restartSupervisor",
};

/** La façon de redémarrer ; une façon inconnue de ce client est dite sans la deviner. */
export function restartText(mode: RestartMode): string {
  return mode in RESTART_KEYS
    ? t(RESTART_KEYS[mode as Exclude<RestartMode, "unknown">])
    : t("serviceDetail.restartUnknown");
}

/** Qui peut joindre un port ; un périmètre inconnu ne s'écrit pas. */
export function reachText(port: ServicePort): string | null {
  if (port.scope === "all") return t("serviceDetail.reachAll");
  if (port.scope === "local") return t("serviceDetail.reachLocal");
  return null;
}

export type MountView =
  | { kind: "folder"; source: string; target: string; access: string }
  | { kind: "volume"; source: string; target: string; access: string }
  | { kind: "other"; source: string; target: string; access: string };

export function mountView(mount: ServiceMount): MountView {
  const access = mount.readOnly ? t("serviceDetail.readOnly") : t("serviceDetail.readWrite");
  const base = { source: mount.source, target: mount.target, access };
  if (mount.kind === "folder") return { kind: "folder", ...base };
  if (mount.kind === "volume") return { kind: "volume", ...base };
  return { kind: "other", ...base };
}

/** « Délai d'arrêt » : le nombre de secondes, accordé. */
export function graceText(seconds: number): string {
  return t(seconds === 1 ? "serviceDetail.graceOne" : "serviceDetail.grace", { n: seconds });
}

/** La commande lancée, en un texte (arguments séparés par une espace) ; contenu sensible, jamais dans un titre ni une infobulle. */
export function commandText(command: readonly string[]): string {
  return command.join(" ");
}

/**
 * « Adopté par {compte} le {date} » (BR-SVC-027). Le nom du compte est un texte de l'agent : affiché seulement
 * s'il a la forme d'un identifiant Hearth, sinon la phrase sans compte.
 */
export function adoptedLine(
  service: ServiceInfo,
  formatDate: (ms: number) => string,
): string | null {
  const adopted = service.adopted;
  if (!adopted) return null;
  const date = adopted.at === null ? null : formatDate(adopted.at);
  const by = isHearthUsername(adopted.by) ? adopted.by : null;
  if (by && date) return t("serviceDetail.adoptedBy", { by, date });
  if (date) return t("serviceDetail.adoptedOn", { date });
  if (by) return t("serviceDetail.adoptedByOnly", { by });
  return null;
}

/** Le détail d'un service adopté dont la commande et les noms de variables sont réservés à ce compte (BR-SVC-028). */
export function isReserved(service: ServiceInfo): boolean {
  return service.detailsReserved;
}
