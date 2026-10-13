---
id: BR-ACCT-017
domaine: ACCT
titre: Un administrateur ferme les sessions de n'importe quel compte, titulaire du droit de gérer les services ou non
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (section 9, BR-ACCT-017), HRT-133
maj: 2026-10-13
---

# BR-ACCT-017 — Fermer les sessions de n'importe quel compte

## Règle
Un administrateur peut fermer les sessions ouvertes de **n'importe quel compte**, y compris un compte qui a le droit de gérer les services (un « titulaire »), que l'administrateur ait ce droit ou non. Fermer des sessions coupe un accès en cours et ne donne rien à celui qui le fait. Le geste reste un acte d'administration du socle (mot de passe et clé du poste, couvert par l'élévation de 5 minutes). Un compte en lecture seule ne ferme les sessions de personne d'autre (BR-ACCT-013).

Avant HRT-133 la route acceptait déjà un titulaire, mais elle faisait aussi oublier ses postes de confiance et ses adresses retenues : un administrateur sans le droit touchait ainsi, par un détour, aux moyens de connexion que BR-GRANT-005 réserve (voir BR-ACCT-018).

## Application (code)
- `crates/hearth-agent/src/domain/sessions.rs::plan_revocation`.
- `crates/hearth-agent/src/application/accounts.rs::AccountService::revoke_sessions_as`.
- Route : `DELETE /api/v1/accounts/{id}/sessions` (`entrypoint/http/accounts.rs::revoke_sessions`, niveau `Admin`).

## Vérification
- `crates/hearth-agent/tests/http_api.rs::an_administrator_without_the_right_closes_the_sessions_of_a_holder_and_nothing_else`, `::closing_your_own_sessions_keeps_the_current_one_and_read_only_cannot_close_any`.

## Règles liées
- BR-ACCT-011, BR-ACCT-013, BR-ACCT-018, BR-ACCT-019, BR-GRANT-005, BR-GRANT-010 (retirer le droit ne ferme pas les sessions : inchangé).

## Historique
- 2026-10-13 : création (HRT-133).
