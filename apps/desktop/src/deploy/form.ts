import type { DeployCheckInput, DeployPort, DeployRequest } from "@/link";
import type { Draft } from "./draft";
import {
  type CommandIssue,
  commandIssue,
  type EnvNameIssue,
  type EnvValueIssue,
  effectiveTag,
  envNameIssue,
  envValueIssue,
  type ImageIssue,
  imageIssue,
  MAX_ENV_VARS,
  MAX_PORT_MAPPINGS,
  type NameIssue,
  nameIssue,
  type PortIssue,
  parsePort,
  splitImage,
  type TagIssue,
  tagIssue,
} from "./rules";

/**
 * Ce qu'on tire d'un brouillon (HRT-69) : les défauts de FORME, la consultation et la demande à envoyer. Des fonctions
 * pures, sans E/S ; les défauts sont des CODES (`deploy/messages.ts` les met en phrases). Les défauts de CONFLIT (nom
 * pris, port tenu) viennent de l'agent, jamais d'ici.
 */

export interface EnvIssues {
  id: number;
  name: EnvNameIssue | null;
  value: EnvValueIssue | null;
}

export interface PortIssues {
  id: number;
  host: PortIssue | "duplicate" | null;
  container: PortIssue | null;
}

export interface FormIssues {
  name: NameIssue | null;
  image: ImageIssue | null;
  tag: TagIssue | null;
  command: CommandIssue | null;
  env: EnvIssues[];
  ports: PortIssues[];
  /** Au moins un défaut de forme dans tout le formulaire. */
  any: boolean;
}

export function formIssues(draft: Draft): FormIssues {
  const names = new Set<string>();
  const env = draft.env.map<EnvIssues>((row) => {
    let name = envNameIssue(row.name);
    if (name === null) {
      if (names.has(row.name)) name = "duplicate";
      names.add(row.name);
    }
    return { id: row.id, name, value: envValueIssue(row.value) };
  });
  const seen = new Set<string>();
  const ports = draft.ports.map<PortIssues>((row) => {
    const host = parsePort(row.host);
    const container = parsePort(row.container);
    let hostIssue: PortIssues["host"] = "issue" in host ? host.issue : null;
    if ("port" in host) {
      const id = `${row.protocol}/${host.port}`;
      if (seen.has(id)) hostIssue = "duplicate";
      seen.add(id);
    }
    return {
      id: row.id,
      host: hostIssue,
      container: "issue" in container ? container.issue : null,
    };
  });
  const issues = {
    name: nameIssue(draft.name),
    image: imageIssue(draft.image),
    tag: tagIssue(draft.tag, draft.image),
    command: commandIssue(draft.command),
    env,
    ports,
  };
  const any =
    issues.name !== null ||
    issues.image !== null ||
    issues.tag !== null ||
    issues.command !== null ||
    env.some((row) => row.name !== null || row.value !== null) ||
    ports.some((row) => row.host !== null || row.container !== null) ||
    draft.env.length > MAX_ENV_VARS ||
    draft.ports.length > MAX_PORT_MAPPINGS;
  return { ...issues, any };
}

/** Les ports DONT la forme est valable, dans l'ordre du formulaire, avec la rangée qui les porte. */
export function readablePorts(draft: Draft): (DeployPort & { id: number })[] {
  const out: (DeployPort & { id: number })[] = [];
  for (const row of draft.ports) {
    const host = parsePort(row.host);
    const container = parsePort(row.container);
    if ("port" in host && "port" in container) {
      out.push({
        id: row.id,
        hostPort: host.port,
        containerPort: container.port,
        protocol: row.protocol,
      });
    }
  }
  return out;
}

/** La version à envoyer : `null` quand aucune n'est saisie (l'agent lit « latest »). */
function tagToSend(draft: Draft): string | null {
  const embedded = splitImage(draft.image).embedded;
  const tag = embedded ?? draft.tag.trim();
  return tag === "" ? null : tag;
}

/**
 * La consultation du formulaire (nom, image, version, ports lisibles), ou `null` s'il n'y a encore rien à demander. Aucun
 * secret : ni variable, ni commande (BR-DEPLOY-033). Un nom de forme invalide est tout de même demandé quand un autre
 * champ l'est : l'agent le dit `invalid`.
 */
export function toCheckInput(draft: Draft): DeployCheckInput | null {
  const ports = readablePorts(draft);
  const askable =
    nameIssue(draft.name) === null || imageIssue(draft.image) === null || ports.length > 0;
  if (!askable) return null;
  return {
    name: draft.name,
    image: draft.image.trim(),
    tag: tagToSend(draft),
    ports: ports.map(({ hostPort, containerPort, protocol }) => ({
      hostPort,
      containerPort,
      protocol,
    })),
  };
}

/**
 * La demande de déploiement, ou `null` si le brouillon a un défaut de forme ou si les ressources ne sont pas réglées
 * (BR-DEPLOY-038 : aucune valeur « sans limite »). `accept` : les points d'attention validés dans le récapitulatif.
 */
export function toRequest(
  draft: Draft,
  accept: { reservedPorts: boolean; lowDisk: boolean },
): DeployRequest | null {
  if (formIssues(draft).any) return null;
  const resources = draft.resources;
  if (resources === null || resources.memoryBytes === null || resources.cpuMillis === null) {
    return null;
  }
  const ports = readablePorts(draft);
  return {
    name: draft.name,
    image: draft.image.trim(),
    tag: tagToSend(draft),
    ports: ports.map(({ hostPort, containerPort, protocol }) => ({
      hostPort,
      containerPort,
      protocol,
    })),
    env: draft.env.map((row) => ({ name: row.name, value: row.value })),
    command: draft.command.trim() === "" ? null : draft.command,
    resources: { memoryBytes: resources.memoryBytes, cpuMillis: resources.cpuMillis },
    accept,
  };
}

/** La version que le récapitulatif montre (« latest » si rien n'est saisi) et si elle vient du défaut (BR-DEPLOY-005). */
export function shownVersion(draft: Draft): { tag: string; byDefault: boolean } {
  return {
    tag: effectiveTag(draft.image, draft.tag),
    byDefault: splitImage(draft.image).embedded === null && draft.tag.trim() === "",
  };
}

/** Le dépôt de l'image à montrer, sans la version écrite dans le champ. */
export function shownRepository(draft: Draft): string {
  return splitImage(draft.image).repository;
}
