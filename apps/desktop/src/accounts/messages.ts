import { type MessageKey, t } from "@/i18n";
import type { AccountRefusal, PasswordRule, Role, UsernameProblem } from "@/link";

/** Les critères d'un mot de passe affichés en direct, dans l'ordre de la spécification. */
export const PASSWORD_CRITERIA: readonly Exclude<PasswordRule, "required">[] = [
  "min_length",
  "digit",
  "lowercase",
  "uppercase",
  "contains_username",
];

const RULE_KEYS: Record<PasswordRule, MessageKey> = {
  required: "accounts.ruleRequired",
  min_length: "accounts.ruleMinLength",
  digit: "accounts.ruleDigit",
  lowercase: "accounts.ruleLowercase",
  uppercase: "accounts.ruleUppercase",
  contains_username: "accounts.ruleContainsUsername",
};

const USERNAME_KEYS: Record<UsernameProblem, MessageKey> = {
  empty: "accounts.usernameEmpty",
  too_short: "accounts.usernameTooShort",
  too_long: "accounts.usernameTooLong",
  invalid_chars: "accounts.usernameInvalidChars",
};

export function ruleText(rule: PasswordRule): string {
  return t(RULE_KEYS[rule]);
}

export function usernameProblemText(problem: UsernameProblem): string {
  return t(USERNAME_KEYS[problem]);
}

export function roleLabel(role: Role): string {
  return t(role === "admin" ? "accounts.roleAdmin" : "accounts.roleReadonly");
}

/**
 * Le texte d'un refus (de l'agent ou de la validation locale). `self` : le compte visé est celui de
 * l'utilisateur (le dernier administrateur ne se supprime pas lui-même, texte propre).
 */
export function refusalMessage(refusal: AccountRefusal, self = false): string {
  switch (refusal.kind) {
    case "invalid_username":
      return usernameProblemText(refusal.problem ?? "invalid_chars");
    case "weak_password": {
      const first = refusal.rules[0];
      return first ? ruleText(first) : t("failure.generic");
    }
    case "username_taken":
      return t("accounts.usernameTaken");
    case "wrong_password":
      return t("accounts.wrongPassword");
    case "last_admin":
      return t(self ? "accounts.lastAdminSelf" : "accounts.lastAdmin");
    case "not_found":
      return t("accounts.notFound");
    case "confirmation_mismatch":
      return t("accounts.confirmationMismatch");
    case "conflict":
      return t("accounts.conflict");
    case "busy":
      return t("accounts.busy");
    case "session_ended":
      return t("accounts.sessionEnded");
    case "session_revoked":
      return t("accounts.sessionRevoked");
    case "other":
      return t("failure.generic");
  }
}
