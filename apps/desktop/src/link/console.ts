import type {
  ConsoleOutcome as BoundConsoleOutcome,
  PlayersOutcome as BoundPlayersOutcome,
  ConsoleSendOutcome as BoundSendOutcome,
} from "@/bindings";
import { oneOf } from "./mods";

/**
 * La console d'un serveur de jeu et les joueurs connectés (HRT-291, ADR-0067, `BR-CONSOLE-001` à `011`) : types de
 * l'interface et conversions depuis les types générés. La classe d'une commande et la politique des verbes viennent de
 * la FICHE du jeu (via l'agent), jamais du client ; l'agent rejuge tout. La réponse du jeu et les pseudos sont du texte
 * hostile : à montrer en texte brut, jamais en HTML ; aucun n'est écrit dans une trace.
 */

/** Pourquoi la console n'est pas ouverte à Hearth sur ce service. */
export type ConsoleUnavailable = "adopted" | "no_console" | "protocol_unsupported" | "unknown";
/** Ce que la console permet maintenant (BR-CONSOLE-006). */
export type ConsoleState = "ready" | "starting" | "stopped" | "busy" | "unknown";

const UNAVAILABLE: readonly ConsoleUnavailable[] = [
  "adopted",
  "no_console",
  "protocol_unsupported",
];
const STATE: readonly ConsoleState[] = ["ready", "starting", "stopped", "busy"];

export interface RefusedVerb {
  verb: string;
  /** `use_hearth_stop`, `use_hearth_restart`, `breaks_server` (ou un motif plus récent, lu « autre »). */
  reason: string;
}

export interface SensitiveVerb {
  verb: string;
  title: string;
  sentence: string;
  /** `first_arg` : le deuxième mot est pris pour `{joueur}`. */
  target: "first_arg" | "none";
}

export interface QuickCommand {
  id: string;
  label: string;
  text: string;
  /** `send` : part comme une commande tapée ; `fill` : posée dans le champ. */
  mode: "send" | "fill";
}

export interface ConsoleView {
  available: boolean;
  unavailableReason: ConsoleUnavailable | null;
  state: ConsoleState;
  /** Le compte a le droit de gérer les services : il peut écrire. */
  canWrite: boolean;
  policy: { refused: RefusedVerb[]; sensitive: SensitiveVerb[] };
  quick: QuickCommand[];
  limits: { maxChars: number; perMinute: number; replyTimeoutS: number } | null;
}

export type ConsoleReadRefusal = { kind: "service_not_found" } | { kind: "other" };

export type ConsoleOutcome =
  | { kind: "ready"; view: ConsoleView }
  | { kind: "refused"; refusal: ConsoleReadRefusal };

export type PlayersState = "ok" | "stopped" | "starting" | "unavailable" | "no_console" | "unknown";
const PLAYERS_STATE: readonly PlayersState[] = [
  "ok",
  "stopped",
  "starting",
  "unavailable",
  "no_console",
];

/** Les joueurs connectés : jamais un chiffre deviné (`online` n'existe que pour `ok`). */
export interface PlayersView {
  state: PlayersState;
  online: number | null;
  max: number | null;
  names: string[];
  /** Des noms comptés mais pas montrés (hors de la forme d'un pseudo). */
  hiddenNames: number;
  /** Des noms au-delà des cent gardés. */
  moreNames: number;
  /** Date de la lecture, RFC 3339 en UTC. */
  readAt: string | null;
}

export type PlayersOutcome =
  | { kind: "ready"; view: PlayersView }
  | { kind: "refused"; refusal: ConsoleReadRefusal };

export type ConsoleReplyKind = "answered" | "empty" | "no_answer" | "unknown";

export interface ConsoleReply {
  outcome: ConsoleReplyKind;
  /** Le texte du jeu, brut ; absent pour `empty` et `no_answer`. */
  reply: string | null;
  truncated: boolean;
}

/** Pourquoi l'agent a refusé une commande (`kind` : une phrase de l'interface). */
export type ConsoleRefusal =
  | { kind: "wrong_password" }
  | { kind: "password_required" }
  | { kind: "too_many_attempts"; retry_after_s: number }
  | { kind: "busy" }
  | { kind: "service_not_found" }
  | { kind: "service_adopted" }
  | { kind: "not_available" }
  | { kind: "invalid"; reason: string | null }
  | { kind: "verb_refused"; reason: string | null }
  | { kind: "class_mismatch" }
  | { kind: "not_running" }
  | { kind: "starting" }
  | { kind: "operation_in_progress" }
  | { kind: "rate_limited"; retry_after_s: number | null }
  | { kind: "unreachable" }
  | { kind: "other" };

export type ConsoleSendOutcome =
  | { kind: "replied"; reply: ConsoleReply }
  | { kind: "refused"; refusal: ConsoleRefusal }
  /** Le lien est tombé avant la réponse : la commande a peut-être été exécutée, elle n'est jamais renvoyée. */
  | { kind: "unknown"; opId: string };

export function toConsoleOutcome(dto: BoundConsoleOutcome): ConsoleOutcome {
  if (dto.kind === "refused") return { kind: "refused", refusal: dto.refusal };
  const view = dto.view;
  return {
    kind: "ready",
    view: {
      available: view.available,
      unavailableReason:
        view.unavailableReason === null ? null : oneOf(UNAVAILABLE, view.unavailableReason),
      state: oneOf(STATE, view.state),
      canWrite: view.canWrite,
      policy: {
        refused: view.policy.refused,
        sensitive: view.policy.sensitive.map((verb) => ({
          ...verb,
          target: verb.target === "first_arg" ? "first_arg" : "none",
        })),
      },
      quick: view.quick.map((quick) => ({
        ...quick,
        mode: quick.mode === "send" ? "send" : "fill",
      })),
      limits: view.limits,
    },
  };
}

export function toPlayersOutcome(dto: BoundPlayersOutcome): PlayersOutcome {
  if (dto.kind === "refused") return { kind: "refused", refusal: dto.refusal };
  const view = dto.view;
  return {
    kind: "ready",
    view: {
      state: oneOf(PLAYERS_STATE, view.state),
      online: view.online,
      max: view.max,
      names: view.names ?? [],
      hiddenNames: view.hiddenNames,
      moreNames: view.moreNames,
      readAt: view.readAt,
    },
  };
}

export function toConsoleSendOutcome(dto: BoundSendOutcome): ConsoleSendOutcome {
  switch (dto.kind) {
    case "replied":
      return {
        kind: "replied",
        reply: {
          outcome: oneOf(["answered", "empty", "no_answer"] as const, dto.reply.outcome),
          reply: dto.reply.reply,
          truncated: dto.reply.truncated,
        },
      };
    case "unknown":
      return { kind: "unknown", opId: dto.opId };
    case "refused":
      return { kind: "refused", refusal: dto.refusal as ConsoleRefusal };
  }
}
