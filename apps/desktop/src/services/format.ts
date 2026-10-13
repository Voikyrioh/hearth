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
  return t("services.portLabel", {
    host: port.hostPort,
    container: port.containerPort,
    protocol: port.protocol.toUpperCase(),
  });
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

/** Le nom accessible d'une ligne : « {nom}, {image}, {état}{, Créé hors de Hearth}{, {n} ports} ». */
export function rowLabel(service: ServiceInfo): string {
  const parts = [service.name, imageLabel(service), t(STATE_KEYS[service.state])];
  if (service.origin === "external") parts.push(t("services.external"));
  const ports = service.ports.length;
  if (ports > 0) {
    parts.push(ports === 1 ? t("services.rowOnePort") : t("services.rowPorts", { n: ports }));
  }
  return parts.join(", ");
}
