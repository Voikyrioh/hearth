import { t } from "@/i18n";
import type { AuditEntry } from "@/link";
import { safeText } from "./text";

/**
 * Mise en forme des valeurs du journal. L'heure s'affiche dans le fuseau du PC (BR-AUDIT-012) ; la
 * source reste en UTC dans l'entrée (`at`) et dans l'export, et se lit dans l'infobulle et le détail.
 */

/** Date et heure locales : `04/10/2026 12:30:15`. `timeZone` : seulement pour les tests. */
export function formatWhen(atMs: number, timeZone?: string): string | null {
  if (!Number.isFinite(atMs)) return null;
  return new Intl.DateTimeFormat("fr-FR", {
    dateStyle: "short",
    timeStyle: "medium",
    ...(timeZone ? { timeZone } : {}),
  }).format(new Date(atMs));
}

/** La date d'une entrée, ou sa source brute (rendue inoffensive) si elle est illisible. */
export function whenOf(entry: AuditEntry, timeZone?: string): string {
  return formatWhen(entry.atMs, timeZone) ?? safeText(entry.at);
}

/** L'origine : le texte de l'agent pour un poste du réseau, un libellé fixe sinon. */
export function originOf(entry: AuditEntry): string {
  if (entry.origin.kind === "cli") return t("audit.originCli");
  if (entry.origin.kind === "assistant") return t("audit.originAssistant");
  return safeText(entry.origin.text);
}

const OUTCOME_KEYS = {
  ok: "audit.outcomeOk",
  denied: "audit.outcomeDenied",
  failed: "audit.outcomeFailed",
} as const;

export function outcomeLabel(outcome: AuditEntry["outcome"]): string {
  return t(OUTCOME_KEYS[outcome]);
}

/** Texte lisible d'une rafale : « 6 tentatives refusées en 2 min ». */
export function burstLabel(attempts: number, minutes: number): string {
  return t("audit.burst", { n: attempts, m: minutes });
}
