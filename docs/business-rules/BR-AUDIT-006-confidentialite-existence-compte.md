---
id: BR-AUDIT-006
domaine: AUDIT
titre: Un échec de connexion ne dit pas si le compte existe
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-006), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-006 — Un échec de connexion ne dit pas si le compte existe

## Règle
L'entrée d'une connexion refusée a la même forme et la même raison (« identifiants incorrects »), que l'identifiant existe ou non, que le mot de passe soit faux ou non : la **raison** ne dit jamais si le compte existe. Un identifiant dont le format est impossible (ce peut être un mot de passe tapé au mauvais endroit) a la raison « identifiant invalide », sans valeur ; cette raison ne dépend que du format. **Le compte visé n'est renseigné que si l'identifiant saisi correspond à un compte existant** : l'administrateur voit alors quel compte est attaqué ; pour un identifiant inconnu ou impossible, le compte est vide (la valeur saisie n'est jamais retenue, BR-AUDIT-005). Cette différence est lisible de l'administrateur seul (le journal ne l'est que de lui) ; elle n'est jamais visible d'un client, dont la réponse reste identique (BR-CONN-013).

## Application (code)
- `crates/hearth-agent/src/domain/audit/event.rs::Reason::{InvalidCredentials, InvalidIdentifier}`.
- `crates/hearth-agent/src/application/sessions.rs::SessionService::login_in_turn`.

## Vérification
- `crates/hearth-agent/tests/audit_use_cases.rs::a_refused_login_is_journaled_without_account_and_says_the_same_thing_for_every_cause`.
- `crates/hearth-agent/tests/audit_https.rs::a_refused_login_is_journaled_without_account_nor_typed_identifier`.
- `domain::audit::event::tests::a_failed_login_never_says_whether_the_account_exists`.

## Cas limites
- Écrite dans la transaction des compteurs de verrouillage : même chemin que BR-CONN-013 (vérification contre un haché factice).

## Règles liées
- BR-CONN-013, BR-AUDIT-005.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
