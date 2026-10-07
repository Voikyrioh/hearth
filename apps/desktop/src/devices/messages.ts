import { t } from "@/i18n";
import type { DeviceRemovalRefusal } from "@/link";

/**
 * Le texte d'un refus de retrait (de l'agent ou de la coquille, avant tout envoi). Choisi d'après
 * `kind` : un refus ne révèle rien d'autre que ce qu'il dit (jamais une clé, un défi, une signature).
 */
export function removalRefusalMessage(refusal: DeviceRemovalRefusal): string {
  switch (refusal.kind) {
    case "wrong_password":
      return t("devices.wrongPassword");
    case "current_device":
      return t("devices.currentHint");
    case "no_device_key":
      return t("devices.notEnrolled");
    case "proof_refused":
      return t("devices.proofRefused");
    case "not_found":
      return t("devices.notFound");
    case "too_many_attempts":
      return t("failure.tooManyAttempts", { n: refusal.retry_after_s });
    case "busy":
      return t("accounts.busy");
    case "unsupported":
      return t("devices.unsupported");
    case "session_ended":
      return t("accounts.sessionEnded");
    case "session_revoked":
      return t("accounts.sessionRevoked");
    case "other":
      return t("failure.generic");
  }
}
