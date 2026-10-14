import { t } from "@/i18n";
import type { LogEnd } from "@/link";
import type { GapKind } from "./buffer";

/**
 * Ce que l'onglet Journaux dit quand la lecture est finie ou n'a pas pu s'ouvrir (HRT-67, BR-SLOG-014 à 018, 020, 026,
 * 027) : des fonctions pures, sans E/S. Une valeur que ce client ne connaît pas (code, fin, état) donne un texte
 * NEUTRE (« La lecture des journaux s'est arrêtée. ») : jamais un état affirmé, jamais un bouton fondé dessus.
 */

export type LogsFailure = "not_connected" | "unreachable";

export interface EndNotice {
  tone: "info" | "warn" | "error";
  text: string;
  /** « Réessayer » : `now` tout de suite, `wait` quand le frein de l'agent demande d'attendre, `none` jamais. */
  retry: "none" | "now" | "wait";
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
        default:
          // `bad_request`, `unknown` : une fin générique, sans rien affirmer de plus.
          return notice("error", t("logs.stopReading"), { retry: "now" });
      }
    case "session_ended":
      return notice("error", t("logs.sessionEnded"));
    case "unsupported":
      return notice("info", t("logs.unsupported"));
    case "unreachable":
      return notice("error", t("logs.failed"), { retry: "now" });
    default:
      return notice("error", t("logs.stopReading"), { retry: "now" });
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
    default:
      if (count === null) return t("logs.gapMissing");
      return t(n === 1 ? "logs.gapMissingOne" : "logs.gapMissingMany", { n });
  }
}

/** « 1 nouvelle ligne » / « 12 nouvelles lignes ». */
export function waitingText(n: number): string {
  return t(n === 1 ? "logs.waitingOne" : "logs.waitingMany", { n });
}
