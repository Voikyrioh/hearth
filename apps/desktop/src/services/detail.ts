import { isHearthUsername } from "@/accounts/name";
import { safeText } from "@/audit/text";
import { type MessageKey, t } from "@/i18n";
import type { RestartMode, ServiceInfo, ServiceMount, ServicePort } from "@/link";
import type { RightsGate } from "@/services/rights";

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
  // Chemins du serveur : rendus inoffensifs (caractères bidirectionnels et invisibles remplacés par un repère).
  const base = { source: safeText(mount.source), target: safeText(mount.target), access };
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
  return safeText(command.join(" "), true);
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

/**
 * Ce que le détail peut dire de la COMMANDE et des NOMS de variables d'un service (BR-SVC-007, 008, 028) :
 * - `visible` : le compte a le droit de gérer les services ET ils ont été lus sur le serveur ;
 * - `reserved` : l'agent les a retirés (service adopté, compte sans le droit) ou le compte n'a pas le droit ;
 * - `right-unknown` : le droit n'est pas (encore) connu : par prudence, rien n'est montré ;
 * - `not-read` : liste relue du disque au redémarrage, écrite sans eux (ils peuvent porter des secrets) : l'absence
 *   n'y dit PAS « aucune variable » ni « commande de l'image ».
 *
 * RÈGLE PRUDENTE, DÉCISION DU PROPRIÉTAIRE ATTENDUE : la commande peut porter un secret (`--token=…`) ; en attendant
 * un filtre côté agent, le client ne montre la commande et les noms de variables qu'aux comptes qui ont le droit de
 * gérer les services, quel que soit le service (BR-SVC-007 et 008 les disent visibles des deux rôles pour un service créé
 * par Hearth : écart assumé, à trancher). L'agent reste l'arbitre pour un service adopté (BR-ADOPT-015).
 */
export type DetailsView = "visible" | "reserved" | "right-unknown" | "not-read";

export function detailsView(service: ServiceInfo, gate: RightsGate): DetailsView {
  if (service.detailsReserved || gate.kind === "denied") return "reserved";
  if (gate.kind !== "allowed") return "right-unknown";
  if (!service.detailsKnown) return "not-read";
  return "visible";
}

/**
 * Qui lit les journaux de ce service (BR-SLOG-011, BR-SLOG-018, BR-ADOPT-015) : un service créé par Hearth, tous les
 * comptes ; un service ADOPTÉ, ceux qui ont le droit de gérer les services. `checking` : le droit est en cours de
 * lecture (rien n'est dit avant) ; `right-unknown` : il n'a pas pu être lu (par prudence, rien n'est ouvert). L'agent
 * reste l'arbitre : ce calcul ne fait que dire d'avance, honnêtement, ce que l'agent va répondre.
 */
export type LogsAccess = "open" | "reserved" | "right-unknown" | "checking";

export function logsAccess(service: Pick<ServiceInfo, "adopted">, gate: RightsGate): LogsAccess {
  if (service.adopted === null) return "open";
  switch (gate.kind) {
    case "allowed":
    // Le pilotage est désactivé sur le serveur : la lecture des journaux est jugée par l'agent seul.
    case "engine-disabled":
      return "open";
    case "reading":
      return "checking";
    case "denied":
      return "reserved";
    default:
      return "right-unknown";
  }
}
