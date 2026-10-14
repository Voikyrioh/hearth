import type { MessageKey } from "@/i18n";
import { t } from "@/i18n";
import type { AttackModeRefusal, AttackModeState } from "@/link";

/**
 * Le libellé de l'état du mode attaque. Tous les états y sont, `unknown` compris (agent plus récent) : un état que
 * ce client ne lit pas se dit « Inconnu », jamais « Actif » ni « Inactif » (BR-COMPAT-001 point 4).
 */
export const ATTACK_STATE_KEYS: Record<AttackModeState, MessageKey> = {
  off: "security.stateOff",
  active: "security.stateOn",
  suspended: "security.stateSuspended",
  unknown: "security.stateUnknown",
};

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
