import { t } from "@/i18n";
import type { AttackModeRefusal } from "@/link";

/**
 * Le texte d'un refus de l'agent à l'activation ou à la désactivation. Choisi d'après `kind` : un refus
 * ne révèle rien d'autre que ce qu'il dit (jamais une clé, un défi, une signature).
 */
export function attackModeRefusalMessage(refusal: AttackModeRefusal): string {
  switch (refusal.kind) {
    case "wrong_password":
      return t("security.wrongPassword");
    case "too_many_attempts":
      return t("failure.tooManyAttempts", { n: refusal.retry_after_s });
    case "busy":
      return t("security.serverBusy");
    case "password_required":
      return t("reauth.elapsed");
    case "unsupported":
      return t("security.unsupported");
    case "session_ended":
      return t("accounts.sessionEnded");
    case "session_revoked":
      return t("accounts.sessionRevoked");
    case "other":
      return t("failure.generic");
  }
}
