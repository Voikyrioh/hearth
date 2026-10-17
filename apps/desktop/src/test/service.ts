import type { ServiceInfo } from "@/link";

/** Un service de la liste avec toutes les valeurs par défaut d'un agent, à compléter par le test (détail compris). */
export function service(patch: Partial<ServiceInfo> & { key: string }): ServiceInfo {
  return {
    name: patch.key,
    origin: "external",
    driver: null,
    unit: null,
    removedOnStop: false,
    composeProject: null,
    state: "running",
    since: null,
    createdAt: null,
    neverStarted: false,
    exitCode: null,
    restarts10m: 0,
    image: { ref: "nginx", tag: "1.27", shortId: "0123456789ab" },
    ports: [],
    portsOmitted: 0,
    hostNetwork: false,
    mounts: [],
    mountsOmitted: 0,
    restart: "unless_stopped",
    envNames: [],
    envNamesOmitted: 0,
    command: null,
    graceS: null,
    adopted: null,
    detailsReserved: false,
    busy: null,
    detailsKnown: true,
    ...patch,
  };
}
