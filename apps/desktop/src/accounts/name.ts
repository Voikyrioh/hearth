/**
 * LA validation d'un nom de compte avant tout affichage (HRT-74 S7, BR-ENGINE-017, BR-GRANT-013). Deux espaces
 * de noms DIFFÉRENTS, deux fonctions, un seul fichier et un seul jeu de tests hostiles :
 * - `isSystemAccountName` : le compte SYSTÈME sous lequel tourne l'agent (aide « Accès refusé à l'agent »,
 *   commande `usermod`). Règle de Linux, pas de Hearth : un compte `pi` est légitime.
 * - `isHearthUsername` : l'identifiant d'un compte HEARTH (commande `hearth-agent account services`).
 *
 * Un nom qui ne passe pas la validation de son usage n'est JAMAIS affiché, ni dans une phrase, un titre, une
 * info-bulle ou un nom accessible, ni dans un bloc à recopier : on ne le tronque pas et on ne le nettoie pas
 * (un nom nettoyé reste un nom faux présenté comme vrai), on n'en parle pas.
 */

/** Hearth : `crates/hearth-proto/src/account_rules.rs`, `USERNAME_MIN_LEN` et `USERNAME_MAX_LEN` (BR-ACCT-002). */
export const HEARTH_USERNAME_MIN = 3;
export const HEARTH_USERNAME_MAX = 32;

/**
 * Nom de compte POSIX (BR-ENGINE-017) : 1 à 32 caractères, commence par une minuscule ASCII ou `_`, puis
 * minuscules, chiffres, `_`, `-`. Le `$` final des comptes de machine n'est pas toléré (le shell l'interprète).
 */
const SYSTEM_ACCOUNT = /^[a-z_][a-z0-9_-]{0,31}$/;

/**
 * Identifiant Hearth, règle exacte de `check_username` (`account_rules.rs`) : 3 à 32 caractères parmi les
 * minuscules ASCII, chiffres, `-` et `_`, sans contrainte sur le premier. Seul refus ajouté : le tiret initial,
 * que la ligne de commande lirait comme une option (aucun autre caractère de la classe n'est dangereux).
 */
const HEARTH_USERNAME = /^[a-z0-9_][a-z0-9_-]{2,31}$/;

export function isSystemAccountName(name: string): boolean {
  return SYSTEM_ACCOUNT.test(name);
}

export function isHearthUsername(name: string): boolean {
  return HEARTH_USERNAME.test(name);
}
