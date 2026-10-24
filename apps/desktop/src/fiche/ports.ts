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

/** Ce qui ne vaut que pour certaines variantes d'une fiche (un port, un dossier) : vide, toutes. */
export function forVariant<T extends { onlyVariants: readonly string[] }>(
  items: readonly T[],
  variant: string | null,
): T[] {
  return items.filter(
    (item) =>
      item.onlyVariants.length === 0 || (variant !== null && item.onlyVariants.includes(variant)),
  );
}

/**
 * Les ports que la fiche publie pour cette variante : ceux de `always`, plus les `optional` que l'utilisateur a cochés.
 * Un port d'une autre variante ne part jamais.
 */
export function publishedPorts(
  ports: readonly FichePort[],
  optionalOn: readonly string[],
  variant: string | null,
): FichePort[] {
  return forVariant(ports, variant).filter(
    (port) =>
      port.publish === "always" || (port.publish === "optional" && optionalOn.includes(port.id)),
  );
}

/** Les ports facultatifs de cette variante : l'utilisateur choisit de les publier ou non. */
export function optionalPorts(ports: readonly FichePort[], variant: string | null): FichePort[] {
  return forVariant(ports, variant).filter((port) => port.publish === "optional");
}

/**
 * Le port du SERVICE pour un port du serveur choisi : celui de la fiche, sauf en `same_number` où le jeu annonce son port
 * et le service, le serveur et le jeu valent le même nombre (BR-FICHE-019).
 */
export function containerPortOf(port: FichePort, hostPort: number): number {
  return port.mapping === "same_number" ? hostPort : port.container;
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
): { port: FichePort; deploy: DeployPort }[] {
  const out: { port: FichePort; deploy: DeployPort }[] = [];
  for (const port of published) {
    const parsed = parseHostPort(hostPorts[port.id] ?? startingHostPort(port));
    if ("port" in parsed) {
      out.push({
        port,
        deploy: {
          hostPort: parsed.port,
          containerPort: containerPortOf(port, parsed.port),
          protocol: port.protocol,
        },
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
  for (const { port, deploy } of readableHostPorts(published, hostPorts)) {
    const id = port.id;
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
