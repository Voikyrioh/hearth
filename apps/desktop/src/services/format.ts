import { formatUptime } from "@/dashboard/format";
import { t } from "@/i18n";
import type { ServiceInfo, ServicePort } from "@/link";

/**
 * Les textes d'une ligne de service (BR-SVC-002, 004, 006, 022), calculés depuis les données et l'heure
 * courante. Aucune règle d'état ici : l'état vient de l'agent (`derive_state`), l'interface le dit.
 */

/** Âge court : « 12 s », « 5 min », « 2 h », « 3 j » (la plus grande unité entière). */
export function shortAge(milliseconds: number): string {
  const seconds = Math.max(0, Math.floor(milliseconds / 1000));
  if (seconds < 60) return t("services.agoSeconds", { n: seconds });
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return t("services.agoMinutes", { n: minutes });
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return t("services.agoHours", { n: hours });
  return t("services.agoDays", { n: Math.floor(hours / 24) });
}

/** `{image}:{version}` ; une image sans nom dit son identifiant court (BR-SVC-022), jamais une version fausse. */
export function imageLabel(service: Pick<ServiceInfo, "image">): string {
  const { ref, tag, shortId } = service.image;
  if (!ref) return t("services.unnamedImage", { id: shortId });
  return tag ? `${ref}:${tag}` : ref;
}

/** « 8080 vers 80 (TCP) ». */
export function portLabel(port: ServicePort): string {
  // Protocole que ce client ne connaît pas : les deux ports, sans protocole inventé.
  if (port.protocol === "unknown") {
    return t("services.portLabelBare", { host: port.hostPort, container: port.containerPort });
  }
  return t("services.portLabel", {
    host: port.hostPort,
    container: port.containerPort,
    protocol: port.protocol.toUpperCase(),
  });
}

/**
 * Largeur estimée d'une puce de port (DM Mono 12 px : 7,2 px par signe, 16 px de marge intérieure) et écart entre
 * deux puces (4 px, seulement ENTRE puces). C'est une estimation de police, pas une mesure.
 */
const CHIP_CHAR = 7.2;
const CHIP_PAD = 16;
const CHIP_GAP = 4;
/**
 * Largeur de la colonne « Ports » selon le mode de la liste (celle des jetons `--svc-columns` : 270 en
 * large, 220 en compact), et nombre de lignes de la ligne de service. La cellule n'a pas de marge propre.
 */
export const PORTS_WIDTH = { wide: 270, compact: 220 } as const;
const PORTS_LINES = 2;
const MORE_WIDTH = 40;

/**
 * Combien de ports la cellule montre : au plus 3 (BR-SVC-002), et seulement ceux qui tiennent sur les deux
 * lignes de la ligne de service avec la puce « +N » quand il en reste ; jamais une puce coupée ni qui déborde.
 * Au moins un port est toujours montré.
 */
export function visiblePortCount(
  ports: readonly ServicePort[],
  columnWidth: number = PORTS_WIDTH.wide,
  omitted = 0,
): number {
  const widths = ports.slice(0, 3).map((port) => portLabel(port).length * CHIP_CHAR + CHIP_PAD);
  for (let shown = widths.length; shown > 1; shown -= 1) {
    const items = widths.slice(0, shown);
    if (ports.length > shown || omitted > 0) items.push(MORE_WIDTH);
    let lines = 1;
    let used = 0;
    for (const width of items) {
      if (used > 0 && used + CHIP_GAP + width > columnWidth) {
        lines += 1;
        used = 0;
      }
      used += (used > 0 ? CHIP_GAP : 0) + width;
    }
    if (lines <= PORTS_LINES) return shown;
  }
  return Math.min(1, ports.length);
}

/** Seconde ligne de la cellule d'état (« depuis 3 j 4 h 12 min », « Arrêté il y a 2 h », « il y a 2 h · code 137 »…). */
export function stateLine(service: ServiceInfo, now: number): string {
  const since = service.since;
  const code = service.exitCode;
  switch (service.state) {
    case "running":
      return since === null
        ? ""
        : t("services.sinceRunning", { duration: formatUptime(Math.max(0, now - since) / 1000) });
    case "stopped":
      if (service.neverStarted) return t("services.neverStarted");
      return since === null ? "" : t("services.stoppedAgo", { age: shortAge(now - since) });
    case "error":
      if (since === null) return code === null ? "" : t("services.loopCode", { code });
      return code === null
        ? t("services.errorAgoNoCode", { age: shortAge(now - since) })
        : t("services.errorAgo", { age: shortAge(now - since), code });
    case "restart_loop":
      return code === null ? "" : t("services.loopCode", { code });
    default:
      return "";
  }
}

/** La bande d'une ligne « Redémarre en boucle » (BR-SVC-006) ; vide pour tout autre état. */
export function loopBand(service: ServiceInfo): string {
  if (service.state !== "restart_loop") return "";
  return service.exitCode === null
    ? t("services.loopBandNoCode", { n: service.restarts10m })
    : t("services.loopBand", { n: service.restarts10m, code: service.exitCode });
}

export const STATE_KEYS = {
  running: "services.state.running",
  stopped: "services.state.stopped",
  starting: "services.state.starting",
  stopping: "services.state.stopping",
  error: "services.state.error",
  restart_loop: "services.state.restartLoop",
  paused: "services.state.paused",
  deploying: "services.state.deploying",
  unknown: "services.state.unknown",
} as const;

/**
 * Tous les ports d'un service, dans un texte (nom accessible, fenêtre des ports, infobulle de souris) : la ligne
 * n'en montre que trois. Ceux que l'agent a coupés au-delà de sa borne sont comptés à la suite, sans être
 * listés (BR-SVC-021).
 */
export function allPortsText(ports: readonly ServicePort[], omitted: number): string {
  const labels = ports.map(portLabel);
  if (omitted > 0) {
    labels.push(
      t(labels.length > 0 ? "services.portsAlsoOmitted" : "services.portsOnlyOmitted", {
        n: omitted,
      }),
    );
  }
  return labels.join(", ");
}

/** Des ports sont-ils cachés derrière « +N » (tous ne tiennent pas, ou l'agent en a coupé) ? */
export function hasHiddenPorts(
  ports: readonly ServicePort[],
  omitted: number,
  columnWidth: number = PORTS_WIDTH.wide,
): boolean {
  return omitted > 0 || ports.length > visiblePortCount(ports, columnWidth, omitted);
}

/** Le nom accessible d'une ligne : « {nom}, {image}, {état}{, Créé hors de Hearth}{, réseau du serveur | {n} ports : liste} ». */
export function rowLabel(service: ServiceInfo): string {
  const parts = [service.name, imageLabel(service), t(STATE_KEYS[service.state])];
  if (service.origin === "external") parts.push(t("services.external"));
  // Les ports coupés par l'agent comptent aussi : le total est un minimum, jamais une liste (BR-SVC-021).
  const ports = service.ports.length + service.portsOmitted;
  if (ports > 0) {
    // La liste COMPLÈTE : ce que la ligne n'a pas la place de montrer reste lisible au lecteur d'écran.
    const count = ports === 1 ? t("services.rowOnePort") : t("services.rowPorts", { n: ports });
    parts.push(`${count} : ${allPortsText(service.ports, service.portsOmitted)}`);
  } else if (service.hostNetwork) {
    parts.push(t("services.hostNetwork"));
  }
  return parts.join(", ");
}
