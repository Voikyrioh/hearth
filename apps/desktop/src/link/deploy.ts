import type { ServiceOperation } from "./serviceOps";
import type { EngineState } from "./services";

/**
 * Déployer une image (HRT-69, ADR-0039, `BR-DEPLOY-*`) : types de l'interface, côté pont. Aucune règle ici : l'agent
 * juge tout, deux fois (BR-DEPLOY-032) ; l'interface ne fait que lui poser les questions et dire ses réponses.
 *
 * - `checkDeployment` est une CONSULTATION (BR-DEPLOY-033) : jamais une variable, un nom de variable ni la commande ;
 * - `deployService` est un acte d'administration confirmé, JAMAIS couvert par l'élévation (le mot de passe est
 *   redemandé à chaque fois), jamais rejoué : un résultat inconnu se lit sur le flux des opérations (BR-DEPLOY-028) ;
 * - `cancelServiceOperation` annule la récupération de l'image (acte couvert par l'élévation) ;
 * - `readEngineCapacity` donne les bornes des curseurs de ressources (BR-DEPLOY-038).
 *
 * La valeur d'une variable, son nom et la commande sont des secrets : ils ne traversent que `deployService`, ne sont
 * ni gardés, ni tracés, ni journalisés par le pont.
 */

export type DeployProtocol = "tcp" | "udp";

/** Une correspondance de ports d'un formulaire (BR-DEPLOY-007). */
export interface DeployPort {
  hostPort: number;
  containerPort: number;
  protocol: DeployProtocol;
}

/** Ce que pose la consultation : le nom, l'image, la version et les ports. Jamais un secret. */
export interface DeployCheckInput {
  name: string;
  image: string;
  /** `null` : version non saisie (« latest »). */
  tag: string | null;
  ports: DeployPort[];
}

/** Une variable d'environnement : le nom ET la valeur sont des secrets pour l'agent. */
export interface DeployEnvVar {
  name: string;
  value: string;
}

/** La demande de déploiement. Les dossiers de données ne sont pas dans ce ticket (HRT-70) : aucun montage n'est envoyé. */
export interface DeployRequest extends DeployCheckInput {
  env: DeployEnvVar[];
  /** La ligne de commande telle que saisie, `null` : la commande de l'image (BR-DEPLOY-014). */
  command: string | null;
  /** Mémoire en octets et processeur en millicœurs : obligatoires (BR-DEPLOY-038). */
  resources: { memoryBytes: number; cpuMillis: number };
  /** Les points d'attention validés dans le récapitulatif (BR-DEPLOY-008, 018). */
  accept: { reservedPorts: boolean; lowDisk: boolean };
}

export type NameStatus = "ok" | "invalid" | "taken" | "being_deleted" | "unknown";

export type PortStatus =
  | "free"
  | "held_by_service"
  | "held_by_unmanaged"
  | "reserved_by_stopped"
  | "held_by_program"
  | "agent_port"
  | "invalid"
  | "unknown";

/** Le constat sur un port du serveur. `holder` : texte du serveur, jamais le nom d'un conteneur non adopté. */
export interface PortCheck {
  hostPort: number;
  protocol: DeployProtocol;
  status: PortStatus;
  holder: string | null;
  /** Le plus petit port libre au-dessus, à défaut le plus grand en dessous (BR-DEPLOY-009). */
  suggested: number | null;
  /** Sous 1024 : avertissement (BR-DEPLOY-007). */
  privileged: boolean;
}

/** La réponse de la consultation. Indicative : le serveur refait tout au lancement (BR-DEPLOY-032). */
export interface DeployCheck {
  name: NameStatus;
  invalidFields: string[];
  ports: PortCheck[];
  /** L'espace libre là où le moteur range ses images ; `null` : l'agent ne peut pas le lire. */
  disk: { freeBytes: number; low: boolean } | null;
  /** L'image est déjà sur le serveur : utilisée telle quelle (BR-DEPLOY-018). */
  imagePresent: boolean;
}

/** Le champ du formulaire qu'un refus de validation désigne (liste fermée de l'agent). */
export type DeployField =
  | "name"
  | "image"
  | "tag"
  | "env"
  | "ports"
  | "mounts"
  | "command"
  | "resources";

/** Pourquoi l'agent a refusé le déploiement, avant tout geste (rien n'est créé). Des CODES, jamais un texte. */
export type DeployRefusal =
  | { kind: "wrong_password" }
  | { kind: "password_required" }
  | { kind: "too_many_attempts"; retry_after_s: number }
  | { kind: "busy" }
  | { kind: "engine_unavailable"; engine: EngineState | null }
  | { kind: "invalid"; field: DeployField; reason: string }
  | { kind: "setting_refused" }
  | { kind: "limits_required" }
  | { kind: "name_taken" }
  | { kind: "name_being_deleted" }
  | {
      kind: "port_in_use";
      port: number;
      protocol: DeployProtocol;
      status: PortStatus;
      holder: string | null;
      suggested: number | null;
    }
  | { kind: "deploy_limit" }
  | { kind: "no_space"; freeBytes: number | null }
  | { kind: "other" };

/**
 * L'issue de l'envoi : accepté (`202`, l'opération court à l'arrière-plan et se suit par `onServiceOperations`),
 * refusé, ou résultat inconnu (le lien est tombé avant la réponse : jamais rejoué).
 */
export type DeployOutcome =
  | { kind: "accepted"; operationId: string; serviceKey: string }
  | { kind: "refused"; refusal: DeployRefusal }
  | { kind: "unknown"; opId: string };

/** Pourquoi l'agent a refusé l'annulation. */
export type CancelRefusal =
  | { kind: "wrong_password" }
  | { kind: "password_required" }
  | { kind: "too_many_attempts"; retry_after_s: number }
  | { kind: "busy" }
  | { kind: "not_found" }
  | { kind: "too_late" }
  | { kind: "not_cancellable" }
  | { kind: "changed" }
  | { kind: "other" };

export type CancelOutcome =
  | { kind: "done"; operation: ServiceOperation }
  | { kind: "refused"; refusal: CancelRefusal }
  | { kind: "unknown"; opId: string };

/** Les champs qu'un refus de validation peut désigner sont connus ; tout autre texte de l'agent devient `other`. */
export const DEPLOY_FIELDS: readonly DeployField[] = [
  "name",
  "image",
  "tag",
  "env",
  "ports",
  "mounts",
  "command",
  "resources",
];
