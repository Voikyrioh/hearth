import { safeText } from "@/audit/text";
import { type MessageKey, t } from "@/i18n";
import type { ConsoleRefusal, ConsoleReply, ConsoleView } from "@/link";
import type { LineError } from "@/services/consoleLine";

/**
 * La console d'un serveur de jeu (HRT-269, `BR-CONSOLE-002` à `010`) : des fonctions pures, sans E/S. Aucun texte ne vient
 * de l'agent, sauf la RÉPONSE DU JEU, montrée en texte brut (jamais du HTML, jamais interprétée) ; l'agent envoie des CODES,
 * l'interface les met en phrases.
 */

/** Pourquoi « Envoyer » est inactif : une ligne hors forme (BR-CONSOLE-003). */
export function lineIssueText(error: LineError, maxChars: number): string {
  switch (error) {
    case "empty":
      return t("console.issue.empty");
    case "too_long":
      return t("console.issue.tooLong", { n: maxChars });
    case "control_char":
      return t("console.issue.control");
  }
}

/** Une ligne tapée avec un retour à la ligne se dit « Une seule ligne à la fois. » avant le caractère à risque. */
export function lineIssueOf(typed: string, error: LineError, maxChars: number): string {
  if (/[\r\n]/.test(typed)) return t("console.issue.oneLine");
  return lineIssueText(error, maxChars);
}

/** Pourquoi le champ est inactif, d'après ce que la console permet maintenant (BR-CONSOLE-006). */
export type FieldBlock =
  | { kind: "stopped" }
  | { kind: "starting" }
  | { kind: "busy" }
  | { kind: "readonly" }
  | { kind: "rate"; seconds: number }
  | { kind: "unavailable" };

export function blockText(block: FieldBlock): { title: string; help: string } {
  switch (block.kind) {
    case "stopped":
      return { title: t("console.block.stopped"), help: t("console.block.stoppedHelp") };
    case "starting":
      return { title: t("console.block.starting"), help: t("console.block.startingHelp") };
    case "busy":
      return { title: t("console.block.busy"), help: t("console.block.busyHelp") };
    case "readonly":
      return { title: t("console.block.readonly"), help: t("console.block.readonlyHelp") };
    case "rate":
      return {
        title: t("console.block.rate"),
        help: t("console.block.rateHelp", { n: block.seconds }),
      };
    case "unavailable":
      return { title: t("console.block.unavailable"), help: t("console.block.unavailableHelp") };
  }
}

/** Ce qui bloque le champ, dans l'ordre : pas de console, droit, état du jeu, débit. */
export function fieldBlock(view: ConsoleView | null, rateWaitS: number): FieldBlock | null {
  if (view === null || !view.available) return { kind: "unavailable" };
  if (!view.canWrite) return { kind: "readonly" };
  if (view.state === "stopped") return { kind: "stopped" };
  if (view.state === "starting") return { kind: "starting" };
  if (view.state === "busy") return { kind: "busy" };
  if (rateWaitS > 0) return { kind: "rate", seconds: rateWaitS };
  return null;
}

const REFUSED_KEYS: Record<string, MessageKey> = {
  use_hearth_stop: "console.refused.stop",
  use_hearth_restart: "console.refused.restart",
  breaks_server: "console.refused.breaks",
};

/** La phrase d'une commande refusée par la fiche : elle dit quoi faire à la place (BR-CONSOLE-004). */
export function refusedSentence(reason: string | null): string {
  return t(REFUSED_KEYS[reason ?? ""] ?? "console.refused.other");
}

/** Les codes de couleur du jeu (`§x`) et les caractères de contrôle d'une réponse ne se montrent pas. */
export function cleanReply(text: string): string {
  return safeText(text.replace(/§./gu, ""), true);
}

/** Une ligne de l'échange de la session : une commande et ce qui lui est arrivé. */
export interface Exchange {
  id: number;
  /** L'heure de l'envoi (ms). */
  at: number;
  /** La commande telle que tapée (texte brut, jamais interprété). */
  command: string;
  /** `answered` : réponse du jeu ; `empty` ; `no_answer` ; `refused` : jamais envoyée ; `unreachable` ; `unknown` ; `failed`. */
  outcome: "answered" | "empty" | "no_answer" | "refused" | "unreachable" | "unknown" | "failed";
  /** Le texte du jeu, brut et nettoyé (`answered`) ; absent sinon. */
  reply: string | null;
  /** La phrase de l'interface qui accompagne (réponse vide, délai, refus...). */
  sentence: string | null;
  /** Le compte qui l'a envoyée (jamais celui d'un autre poste : l'échange vit dans cet écran). */
  by: string | null;
}

/** Ce que dit l'interface d'une réponse du jeu. */
export function replyExchange(
  reply: ConsoleReply,
): Pick<Exchange, "outcome" | "reply" | "sentence"> {
  switch (reply.outcome) {
    case "answered": {
      const text = reply.reply === null ? "" : cleanReply(reply.reply);
      return {
        outcome: text === "" ? "empty" : "answered",
        reply: text === "" ? null : text,
        sentence:
          text === ""
            ? t("console.reply.empty")
            : reply.truncated
              ? t("console.reply.truncated")
              : null,
      };
    }
    case "empty":
      return { outcome: "empty", reply: null, sentence: t("console.reply.empty") };
    case "no_answer":
      return { outcome: "no_answer", reply: null, sentence: t("console.reply.noAnswer") };
    default:
      return { outcome: "failed", reply: null, sentence: t("console.reply.unknown") };
  }
}

/** La phrase d'un refus de l'agent sur une commande (hors confirmation, dite sous le champ du mot de passe). */
export function refusalSentence(refusal: ConsoleRefusal): string {
  switch (refusal.kind) {
    case "invalid":
      return t("console.refusal.invalid");
    case "verb_refused":
      return refusedSentence(refusal.reason);
    case "class_mismatch":
      return t("console.refusal.classMismatch");
    case "not_running":
      return t("console.block.stopped");
    case "starting":
      return t("console.block.starting");
    case "operation_in_progress":
      return t("console.block.busy");
    case "rate_limited":
      return t("console.block.rateHelp", { n: refusal.retry_after_s ?? 60 });
    case "unreachable":
      return t("console.reply.unreachable");
    case "service_not_found":
    case "service_adopted":
    case "not_available":
      return t("console.refusal.gone");
    default:
      return t("console.refusal.other");
  }
}
