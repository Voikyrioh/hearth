import { commands } from "@/bindings";
import { t } from "@/i18n";
import { useToastsStore } from "@/stores/toasts";

/** Taille maximale d'un message envoyé au journal (la coquille Rust borne aussi). */
export const MAX_REPORT_CHARS = 2000;
/** Au plus `MAX_REPORTS` rapports par `WINDOW_MS` : une boucle d'erreurs ne remplit pas le journal. */
export const MAX_REPORTS = 10;
export const WINDOW_MS = 10_000;

let windowStart = 0;
let sent = 0;

/** Remise à zéro du débit (tests). */
export function resetReportRate() {
  windowStart = 0;
  sent = 0;
}

function describe(error: unknown): string {
  if (error instanceof Error) return `${error.name}: ${error.message}\n${error.stack ?? ""}`;
  try {
    return typeof error === "string" ? error : (JSON.stringify(error) ?? String(error));
  } catch {
    return String(error);
  }
}

function allowed(now: number): boolean {
  if (now - windowStart >= WINDOW_MS) {
    windowStart = now;
    sent = 0;
  }
  sent += 1;
  return sent <= MAX_REPORTS;
}

/**
 * Écrit l'erreur au journal du client (une ligne, bornée en taille et en débit), sans aucune
 * notification. Ne lève jamais. Sert quand l'appelant montre déjà son propre message clair.
 */
export function logUiError(error: unknown, source: string): void {
  if (!allowed(Date.now())) return;
  const message = describe(error).slice(0, MAX_REPORT_CHARS);
  try {
    // Hors de l'application (navigateur de revue), la commande n'existe pas : on ignore.
    void commands.logFrontendError(source, message).catch(() => {});
  } catch {
    // Pont Tauri absent : sans effet.
  }
}

/**
 * Une erreur de l'interface (composant, rejet de promesse, erreur globale) : notification
 * discrète (compteur si elle se répète, jamais d'écran bloquant) puis ligne dans le
 * journal du client. Ne lève jamais : rapporter un échec ne doit pas en causer un autre.
 */
export function reportUiError(error: unknown, source: string): void {
  try {
    useToastsStore().push({ kind: "error", message: t("toast.uiError") });
  } catch {
    // Pas de magasin (démarrage très précoce) : le journal reste tenté ci-dessous.
  }
  logUiError(error, source);
}
