import { type MessageKey, t } from "@/i18n";
import type { LogEnd, LogGapReason, LogSource, LogState } from "@/link";
import type { GapKind } from "./buffer";

/**
 * Ce que l'onglet Journaux dit quand la lecture est finie ou n'a pas pu s'ouvrir (HRT-67, BR-SLOG-014 à 018, 020, 026,
 * 027) : des fonctions pures, sans E/S. Une valeur que ce client ne connaît pas (code, fin, état) donne un texte
 * NEUTRE (« La lecture des journaux s'est arrêtée. ») : jamais un état affirmé, jamais un bouton fondé dessus.
 */

export type LogsFailure = "not_connected" | "unreachable" | "reads_full";

export interface EndNotice {
  tone: "info" | "warn" | "error";
  text: string;
  /** « Réessayer » : `now` tout de suite, `wait` quand le frein de l'agent demande d'attendre, `none` jamais. */
  retry: "none" | "now" | "wait";
  /** Le mot du bouton : « Réessayer », ou « Reprendre » quand la lecture a été arrêtée par la coquille. */
  retryWord: "retry" | "resume";
  /** L'avis est une alerte (lue tout de suite) plutôt qu'une information. */
  alert: boolean;
  /** Les lignes déjà affichées sont grisées. */
  stale: boolean;
  /** Une icône de verrou : la lecture est réservée. */
  lock: boolean;
}

const notice = (
  tone: EndNotice["tone"],
  text: string,
  options: Partial<Omit<EndNotice, "tone" | "text">> = {},
): EndNotice => ({
  tone,
  text,
  retry: "none",
  retryWord: "retry",
  alert: tone === "error",
  stale: false,
  lock: false,
  ...options,
});

/**
 * L'avis d'une lecture qui n'est pas (ou plus) en cours. `remainingS` : secondes restantes du frein, 0 une fois
 * écoulé. `null` : rien à dire.
 */
export function endNotice(input: {
  end: LogEnd | null;
  failure: LogsFailure | null;
  hasLines: boolean;
  remainingS: number;
}): EndNotice | null {
  const { end, failure, hasLines, remainingS } = input;
  if (failure === "not_connected") {
    return notice("info", t("logs.notConnected"), { retry: "now" });
  }
  if (failure === "unreachable") return notice("error", t("logs.failed"), { retry: "now" });
  // La coquille refuse faute de place : on dit pourquoi et quoi faire, jamais « impossible » avec un bouton sans effet.
  if (failure === "reads_full") return notice("warn", t("logs.readsFull"), { retry: "now" });
  if (end === null) return null;
  switch (end.kind) {
    case "refused":
      switch (end.refusal) {
        case "logs_account_limit":
          return notice("error", t("logs.accountLimit"), { retry: "now" });
        case "logs_limit":
          return notice("error", t("logs.serverLimit"), { retry: "now" });
        case "logs_rate_limited":
          return notice(
            "error",
            remainingS > 0 ? rateLimitedText(remainingS) : t("logs.rateLimitedDone"),
            { retry: "wait" },
          );
        case "logs_unreadable":
          return notice("info", t("logs.unreadable"));
        case "missing_right":
          return notice("info", t("serviceDetail.reserved"), { lock: true });
        case "service_not_found":
          return notice("info", hasLines ? t("logs.gone") : t("serviceDetail.gone"), {
            stale: true,
          });
        case "engine_unavailable":
          return notice("warn", t("logs.engineNotReady"), { retry: "now", stale: true });
        case "bad_request":
          return notice("error", t("logs.stopReading"), { retry: "now" });
        case "unknown":
          // Un code d'un agent plus récent : une fin neutre, sans rien affirmer ni proposer de relance
          // (BR-COMPAT-001 point 4). La lecture ne repart jamais seule.
          return notice("info", t("logs.stopReading"));
        default:
          return unknownEnd();
      }
    case "session_ended":
      return notice("error", t("logs.sessionEnded"));
    case "unsupported":
      return notice("info", t("logs.unsupported"));
    case "unreachable":
      // Plusieurs reprises de suite sans réponse : la lecture ne repart plus seule, le bouton relance.
      return notice("error", t("logs.gaveUp"), { retry: "now" });
    case "closed":
      return notice("info", closedText(end.cause), { retry: "now", retryWord: "resume" });
    case "unknown":
    default:
      return unknownEnd();
  }
}

/** Ce qui est arrivé à une lecture que la coquille a fermée. Les lignes ont été effacées dans tous les cas. */
function closedText(cause: Extract<LogEnd, { kind: "closed" }>["cause"]): string {
  switch (cause) {
    case "window_hidden":
      return t("logs.closedHidden");
    case "server_changed":
      return t("logs.closedServer");
    default:
      return t("logs.closedIdle");
  }
}

/** Une fin que ce client ne sait pas lire : un message neutre, aucune relance. */
const unknownEnd = (): EndNotice => notice("info", t("logs.stopReading"));

/** L'état du service lu, en mots (`unknown` : un état d'un agent plus récent, rien n'est affirmé). */
export const LOG_STATE_KEYS: Record<LogState, MessageKey> = {
  live: "logs.live",
  ended: "logs.stopped",
  waiting: "logs.waiting",
  unknown: "logs.stateUnknown",
};

/**
 * Le repère d'une ligne : `error` pour la sortie d'erreur, `unknown` pour une source que ce client ne connaît pas
 * (ni erreur ni sortie normale : un repère neutre), `null` pour la sortie normale.
 */
export function sourceTag(source: LogSource): "error" | "unknown" | null {
  if (source === "err") return "error";
  if (source === "unknown") return "unknown";
  return null;
}

/** Le genre de trou d'un message `gap` : une raison inconnue n'est pas prise pour « le moteur ne les a pas gardées ». */
export function gapKindOf(reason: LogGapReason): GapKind {
  switch (reason) {
    case "slow":
      return "slow";
    case "unknown":
      return "unknown";
    default:
      return "missing";
  }
}

export function rateLimitedText(seconds: number): string {
  return t(seconds === 1 ? "logs.rateLimitedOne" : "logs.rateLimited", { n: seconds });
}

/** Le code de sortie en mots, dans une phrase (« code 137 (coupé de force...) »). */
export function exitWords(code: number): string {
  const explanation =
    code === 0
      ? t("serviceDetail.exit0")
      : code === 137
        ? t("serviceDetail.exit137")
        : code === 143
          ? t("serviceDetail.exit143")
          : null;
  return explanation === null
    ? t("logs.codeBare", { code })
    : t("logs.codeWords", { code, explanation });
}

/** Le texte d'un repère de trou. `count` : nombre de lignes manquantes quand l'agent le sait. */
export function gapText(kind: GapKind, count: number | null): string {
  const n = count ?? 0;
  switch (kind) {
    case "slow":
      return count === null
        ? t("logs.gapSlow")
        : t(n === 1 ? "logs.gapSlowOne" : "logs.gapSlowMany", { n });
    case "pause":
      return t(n === 1 ? "logs.gapPauseOne" : "logs.gapPauseMany", { n });
    case "unknown":
      if (count === null) return t("logs.gapUnknown");
      return t(n === 1 ? "logs.gapMissingOne" : "logs.gapMissingMany", { n });
    default:
      if (count === null) return t("logs.gapMissing");
      return t(n === 1 ? "logs.gapMissingOne" : "logs.gapMissingMany", { n });
  }
}

/** « 1 nouvelle ligne » / « 12 nouvelles lignes ». */
export function waitingText(n: number): string {
  return t(n === 1 ? "logs.waitingOne" : "logs.waitingMany", { n });
}
