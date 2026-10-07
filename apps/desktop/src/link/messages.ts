import { t } from "@/i18n";
import { LinkCommandError, type LinkFailure } from "./types";

/** L'échec d'une commande, s'il en est un (une `LinkCommandError`), sinon `null`. */
export function failureOf(error: unknown): LinkFailure | null {
  return error instanceof LinkCommandError ? error.failure : null;
}

/**
 * Le texte d'un échec de commande, tel qu'on le montre à l'utilisateur. Un échec de connexion ne
 * dit jamais lequel de l'identifiant ou du mot de passe est faux (BR-CONN-013). `seconds` :
 * attente restante pour « trop de tentatives » (le compte à rebours de l'écran).
 */
export function failureMessage(failure: LinkFailure, seconds?: number): string {
  switch (failure.kind) {
    case "unreachable":
      return t("failure.unreachable");
    case "not_agent":
      return t("failure.notAgent");
    case "incompatible_agent":
      return t("failure.agentTooOld");
    case "incompatible_client":
      return t("failure.clientTooOld");
    case "invalid_credentials":
      return t("failure.invalidCredentials");
    case "too_many_attempts":
      return t("failure.tooManyAttempts", { n: seconds ?? failure.retry_after_s });
    case "name_taken":
      return t("validation.nameTaken");
    case "already_exists":
      return t("connect.existing");
    case "invalid_input":
      return failure.field === "name"
        ? t("validation.nameRequired")
        : failure.field === "address"
          ? t("validation.hostInvalid")
          : failure.field === "port"
            ? t("validation.portInvalid")
            : t("failure.generic");
    case "storage":
      return t("failure.storage");
    case "vault":
      return t("failure.vault");
    case "tracking_unavailable":
      return t("failure.trackingUnavailable");
    case "tracking_slow":
      return t("failure.trackingSlow");
    case "not_connected":
      return t("failure.notConnected");
    case "forbidden":
      return t("failure.forbidden");
    case "not_recognized":
      return t("security.notEnrolled");
    case "device_challenge_unavailable":
      return t("failure.deviceChallengeUnavailable");
    case "unknown_server":
      return t("failure.unknownServer");
    case "fingerprint_changed":
    case "verification_required":
    case "internal":
      return t("failure.generic");
  }
}
