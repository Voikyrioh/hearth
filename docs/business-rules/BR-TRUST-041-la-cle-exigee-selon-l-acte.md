---
id: BR-TRUST-041
domaine: TRUST
titre: Clé exigée (Q18) : retrait d'un poste, la clé du poste courant ; mode attaque et autres actes, une clé inscrite du compte appelant
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-07-technique-administration-mot-de-passe-et-cle.md ; contexts/hearth/tickets/hrt/HRT-28.md
maj: 2026-10-07
---

# BR-TRUST-041 : Clé exigée (Q18) : retrait d'un poste, la clé du poste courant ; mode attaque et autres actes, une clé inscrite du compte appelant

## Règle
| Acte | Clé exigée | Contrat |
|---|---|---|
| Retrait d'un poste | clé du **poste courant** (relié à la session, BR-TRUST-048) | livré, usage `0x04`, inchangé |
| Mode attaque | une clé inscrite du compte appelant | `0x05` (la forme à plat `0x03` reste acceptée) |
| Autres actes (création, rôle, mot de passe d'un compte, suppression, fermeture des sessions, mise à jour de l'agent, son propre mot de passe, réglage) | une clé inscrite du compte appelant | `0x05` |

La clé d'un autre poste inscrit du même compte est acceptée pour le mode attaque et les actes nouveaux ; la clé d'un autre compte est refusée (`proof_invalid`).

## Application (code)
- `application/trust.rs::TrustService::{verify_act, verify_attack_mode, verify_removal}`.

## Vérification
- `tests/admin_reauth.rs::the_key_of_another_enrolled_device_of_the_account_is_accepted_and_the_key_of_another_account_is_not` ; `tests/device_http.rs`, `tests/attack_mode_http.rs` inchangés.

## Règles liées
- BR-TRUST-022, 028, 048, ADR-0031.

## Historique
- 2026-10-07 : création (HRT-28, tranche A ; décision Q18).
- 2026-10-07 (HRT-30) : l'interface (`security/gate.ts`) n'exige plus une session « prouvée » pour le mode attaque : l'agent accepte la preuve de TOUTE clé inscrite du compte (Q18), seule la clé au coffre de ce PC est vérifiée côté client ; une clé que l'agent ne connaîtrait pas est refusée par lui (`not_recognized`, rien n'est modifié).