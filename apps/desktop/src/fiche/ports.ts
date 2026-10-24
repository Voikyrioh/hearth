import type { DeployPort, FichePort } from "@/link";

/**
 * Les ports d'une fiche (HRT-102, BR-FICHE-019), des fonctions pures sans E/S. Le port du jeu de la fiche est proposé,
 * l'utilisateur peut le changer ; un port facultatif se publie sur demande ; un port « jamais » n'est pas publié. Les
 * conflits (qui tient quoi) sont jugés par l'agent (`POST /deployments/check`), jamais ici.
 */

/** Une fiche ne publie jamais un port sous 1024 (BR-FICHE-007, BR-DEPLOY-007). */
export const FICHE_PORT_MIN = 1024;
export const FICHE_PORT_MAX = 65535;

export type HostPortIssue = "required" | "not_a_number" | "range";

/** Les ports que la fiche publie : ceux de `always`, plus les `optional` que l'utilisateur a cochés. */
export function publishedPorts(
  ports: readonly FichePort[],
  optionalOn: readonly string[],
): FichePort[] {
  return ports.filter(
    (port) =>
      port.publish === "always" || (port.publish === "optional" && optionalOn.includes(port.id)),
  );
}

/** Les ports facultatifs : l'utilisateur choisit de les publier ou non. */
export function optionalPorts(ports: readonly FichePort[]): FichePort[] {
  return ports.filter((port) => port.publish === "optional");
}

/** Le port du serveur proposé par la fiche, ou vide quand elle n'en propose pas. */
export function startingHostPort(port: FichePort): string {
  return port.defaultHost === null ? "" : String(port.defaultHost);
}

export function parseHostPort(text: string): { port: number } | { issue: HostPortIssue } {
  const trimmed = text.trim();
  if (trimmed === "") return { issue: "required" };
  if (!/^\d+$/.test(trimmed)) return { issue: "not_a_number" };
  const port = Number(trimmed);
  if (port < FICHE_PORT_MIN || port > FICHE_PORT_MAX) return { issue: "range" };
  return { port };
}

/**
 * Les ports lisibles de la saisie, prêts pour la consultation du serveur et pour la demande : le port du serveur choisi,
 * le port du service, le protocole.
 */
export function readableHostPorts(
  published: readonly FichePort[],
  hostPorts: Readonly<Record<string, string>>,
): { id: string; deploy: DeployPort }[] {
  const out: { id: string; deploy: DeployPort }[] = [];
  for (const port of published) {
    const parsed = parseHostPort(hostPorts[port.id] ?? startingHostPort(port));
    if ("port" in parsed) {
      out.push({
        id: port.id,
        deploy: { hostPort: parsed.port, containerPort: port.container, protocol: port.protocol },
      });
    }
  }
  return out;
}

/** Les ports publiés qui portent le même numéro et le même protocole qu'un autre : leurs identifiants. */
export function duplicateHostPorts(
  published: readonly FichePort[],
  hostPorts: Readonly<Record<string, string>>,
): Set<string> {
  const seen = new Map<string, string>();
  const dup = new Set<string>();
  for (const { id, deploy } of readableHostPorts(published, hostPorts)) {
    const key = `${deploy.protocol}/${deploy.hostPort}`;
    const other = seen.get(key);
    if (other !== undefined) {
      dup.add(other);
      dup.add(id);
    }
    seen.set(key, id);
  }
  return dup;
}
