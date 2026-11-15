import { t } from "@/i18n";
import type { DepositRefusal, Transfer, TransferRefusal, TransferStatus } from "@/link";

/**
 * Les textes et les règles du panneau « Transferts » (HRT-96, `BR-FILES-019`, `BR-FILES-013`) : des fonctions pures,
 * sans E/S. Aucun texte ne vient de l'agent : il envoie des CODES, l'interface les met en phrases. Un code qu'elle ne
 * connaît pas ne devient jamais une phrase inventée.
 */

const KIB = 1024;
const MIB = KIB * 1024;
const GIB = MIB * 1024;

/** Une taille lisible, en octets binaires (« 600 Mio », « 1,4 Gio ») : une décimale seulement au-dessus de 10 %. */
export function formatSize(bytes: number): string {
  const decimal = (value: number, unit: string) => {
    const text = value >= 10 ? Math.round(value).toString() : value.toFixed(1).replace(".", ",");
    return `${text.replace(".0", "").replace(",0", "")} ${unit}`;
  };
  if (bytes >= GIB) return decimal(bytes / GIB, t("transfers.units.gib"));
  if (bytes >= MIB) return decimal(bytes / MIB, t("transfers.units.mib"));
  if (bytes >= KIB) return decimal(bytes / KIB, t("transfers.units.kib"));
  return `${Math.max(0, Math.round(bytes))} ${t("transfers.units.byte")}`;
}

/** La part envoyée, entre 0 et 1 ; `null` quand la taille n'est pas connue. */
export function ratioOf(transfer: Transfer): number | null {
  if (!(transfer.size > 0)) return null;
  return Math.min(1, Math.max(0, transfer.sent / transfer.size));
}

/** « 420 Mio sur 600 Mio ». */
export function progressText(transfer: Transfer): string {
  return t("transfers.progress", {
    sent: formatSize(transfer.sent),
    total: formatSize(transfer.size),
  });
}

/** Un transfert qui envoie ou qui attend une reprise : il reste dans la liste. */
export function isOpen(status: TransferStatus): boolean {
  return status === "sending" || status === "interrupted";
}

/** « Reprendre l'envoi » est un geste, jamais automatique : seulement d'un envoi interrompu. */
export function canResume(transfer: Transfer): boolean {
  return transfer.status === "interrupted" && transfer.refusal?.kind !== "gone";
}

/** Annuler : un envoi qui envoie ou qui attend sa reprise (la partie reçue est retirée du serveur). */
export function canCancel(transfer: Transfer): boolean {
  return isOpen(transfer.status);
}

/** Retirer de la liste : un transfert fini, jamais un transfert encore ouvert. */
export function canDismiss(transfer: Transfer): boolean {
  return !isOpen(transfer.status);
}

/** La phrase d'un refus du serveur pendant l'envoi. */
export function refusalText(refusal: TransferRefusal): string {
  switch (refusal.kind) {
    case "no_space":
      return t("transfers.refusal.noSpace", {
        missing: formatSize(refusal.missing),
        remaining: formatSize(refusal.remaining),
      });
    case "too_many":
      return t("transfers.refusal.tooMany");
    case "missing_right":
      return t("transfers.refusal.missingRight");
    case "rejected":
      return t("transfers.refusal.rejected");
    case "source_unreadable":
      return t("transfers.refusal.sourceUnreadable");
    case "source_changed":
      return t("transfers.refusal.sourceChanged");
    case "gone":
      return t("transfers.refusal.gone");
  }
}

/** La phrase d'état d'un transfert : son état, puis la raison quand il y en a une. */
export function statusText(transfer: Transfer): string {
  if (transfer.refusal !== null && transfer.status !== "done") {
    return refusalText(transfer.refusal);
  }
  return t(`transfers.status.${transfer.status}`);
}

/** La phrase d'un refus à l'OUVERTURE du dépôt (avant tout envoi). */
export function depositRefusalText(refusal: DepositRefusal): string {
  switch (refusal.kind) {
    case "wrong_password":
      return t("transfers.deposit.wrongPassword");
    case "password_required":
      return t("transfers.deposit.passwordRequired");
    case "too_many_attempts":
      return t("transfers.deposit.tooManyAttempts", { seconds: Math.ceil(refusal.waitS) });
    case "busy":
      return t("transfers.deposit.busy");
    case "no_space":
      return t("transfers.refusal.noSpace", {
        missing: formatSize(refusal.missing),
        remaining: formatSize(refusal.remaining),
      });
    case "too_many":
      return t("transfers.deposit.tooMany");
    case "name_invalid":
      return t("transfers.deposit.nameInvalid");
    case "source_unreadable":
      return t("transfers.deposit.sourceUnreadable");
    case "other":
      return t("transfers.deposit.other");
  }
}
