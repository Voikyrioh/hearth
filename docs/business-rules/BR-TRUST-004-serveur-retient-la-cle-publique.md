---
id: BR-TRUST-004
domaine: TRUST
titre: Le serveur retient la clé publique de chaque appareil reconnu, inscrite dans la transaction de la connexion par mot de passe réussie
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md ; conception technique 2026-10-06 ; contexts/hearth/tickets/hrt/HRT-22.md ; ADR-0023
maj: 2026-10-07
---

# BR-TRUST-004 : Le serveur retient la clé publique de chaque appareil reconnu, inscrite dans la transaction de la connexion par mot de passe réussie

## Règle
Le serveur retient la clé **publique** de chaque appareil reconnu pour le reconnaître aux connexions suivantes. Un poste est inscrit **seulement** par une connexion par mot de passe accordée, dans la transaction qui ouvre la session, si la signature est valide sous la clé fournie, que la clé n'est inscrite pour aucun autre compte, que le compte a moins de 8 postes (BR-TRUST-022) et que le mode attaque n'est pas actif. **Jamais par une session seule** : une session volée ne peut pas s'offrir une clé.

- Une même clé présentée deux fois est un seul poste (`proven`) ; sa dernière preuve est datée.
- Une clé de petit ordre n'est jamais inscrite (BR-TRUST-005). La clé inscrite est comparée à celle de la preuve (à temps constant), pas seulement son empreinte. Le défi n'est consommé qu'au moment où l'inscription (ou la preuve sous une clé inscrite) sert.
- Seule la clé publique est gardée (32 octets), avec son empreinte (`key_id`, 16 octets de SHA-256 en hexadécimal) ; jamais une clé privée.
- Le nom du poste est celui de `X-Hearth-Client`, nettoyé (jamais un texte libre au journal). Journal : `device.enroll`, sans cible ni champ libre.
- La clé ne change **aucune décision d'accès** dans cette version (BR-TRUST-035 : en état normal, aucune règle n'est appliquée ; la règle 2 sur 3 est HRT-24).
- Le refus d'inscrire n'est jamais une erreur : la réponse porte `device` = `enrolled`, `proven`, `limit`, `deferred`, ou rien.

## Application (code)
- `crates/hearth-agent/src/domain/trust/device.rs::{judge_enrollment, Enrollment, device_name}` (la décision).
- `crates/hearth-agent/src/application/trust.rs::TrustService::on_login` ; appelée par `application/sessions.rs::SessionService::login_in_turn` après le succès, dans la transaction de la connexion (seul appelant).
- `crates/hearth-agent/src/infrastructure/sqlite/device_repo.rs` ; migration `0005_trusted_devices_attack_mode.sql` (table `trusted_devices`).

## Vérification
- Domaine : `domain::trust::device::tests::the_truth_table_of_the_enrollment_is_exhaustive`, `…::the_device_name_is_cleaned_bounded_and_never_empty`.
- `tests/device_proof.rs` : `a_valid_proof_with_the_right_password_enrolls_the_device_and_learns_its_address`, `the_same_key_presented_twice_is_one_device_proven_the_second_time`, `a_session_alone_never_enrolls_a_device_nor_retains_an_address`, `an_enrolment_is_atomic_with_the_login_nothing_is_left_when_the_login_is_not_granted`, `a_key_enrolled_for_one_account_is_never_enrolled_for_another`, `the_enrolment_is_frozen_while_the_attack_mode_is_active`, `a_wrong_password_looks_the_same_with_or_without_a_proof_for_an_existing_or_missing_account`, `the_journal_of_the_devices_has_no_key_no_secret_and_says_nothing_of_missing_identifiers`.
- `tests/migration_0005.rs` : les quatre tests de la migration.

## Cas limites
- Une clé inscrite pour un autre compte n'est pas inscrite ici (`device` absent) ; la contrainte d'unicité est par compte, la recherche de la clé est tous comptes confondus.
- Inscription gelée pendant le mode attaque (`deferred`) : la ligne `attack_mode` est lue ; HRT-25 l'écrira.
- Base restaurée, poste retiré ou compte recréé : la connexion suivante par mot de passe réinscrit sans rien demander.

## Règles liées
- BR-TRUST-003, BR-TRUST-005, BR-TRUST-022, BR-TRUST-035, BR-CONN-013, BR-UPDATE-029, ADR-0023.

## Historique
- 2026-10-07 : création (HRT-22, session 2026-10-04-hearth-creation, T32).
