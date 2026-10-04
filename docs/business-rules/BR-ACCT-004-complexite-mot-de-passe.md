---
id: BR-ACCT-004
domaine: ACCT
titre: Le mot de passe fait au moins 12 caractères avec majuscule, minuscule et chiffre
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-004), HRT-03
maj: 2026-10-04
---

# BR-ACCT-004 — Le mot de passe fait au moins 12 caractères avec majuscule, minuscule et chiffre

## Règle
Minimum 12 caractères, dont au moins une majuscule, une minuscule et un chiffre. La fonction rend **toutes** les règles non respectées (pour l'affichage en direct), dans l'ordre : longueur, chiffre, minuscule, majuscule, identifiant. Un mot de passe vide ne rend que « Le mot de passe est requis ». Le mot de passe est ensuite haché en Argon2id (m = 19 Mio, t = 2, p = 1).

## Application (code)
- `crates/hearth-agent/src/domain/accounts/password.rs::unmet_rules` — liste pure des règles non respectées.
- `crates/hearth-agent/src/domain/accounts/password.rs::PlainPassword::new` — seul moyen de fabriquer un mot de passe à hacher ; refuse avec `PasswordRejected { rules }`.
- `crates/hearth-agent/src/infrastructure/argon2.rs::Argon2Hasher` — hachage (hors du runtime asynchrone).

## Vérification
- Tests : `domain::accounts::password::tests` (limite 11/12, chaque classe, ordre, longueur en caractères) ; `infrastructure::argon2::tests`.

## Cas limites
- Longueur comptée en caractères (un « É » compte pour un).
- Majuscule et minuscule au sens Unicode ; chiffre = 0 à 9.
- Le mot de passe actuel d'un compte (vérification de l'ancien) n'est pas soumis à ces règles : il a pu être défini sous des règles différentes.

## Règles liées
- BR-ACCT-005, BR-ACCT-006.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
