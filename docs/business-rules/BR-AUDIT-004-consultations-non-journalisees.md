---
id: BR-AUDIT-004
domaine: AUDIT
titre: Les consultations ne sont pas journalisées
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-004), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-004 — Les consultations ne sont pas journalisées

## Règle
Afficher le tableau de bord, le journal lui-même, l'état du serveur ou la liste des comptes ne produit aucune entrée quand la demande aboutit. Seul un **refus faute de droits** est consigné, consultation comprise (BR-AUDIT-021).

## Application (code)
- `crates/hearth-agent/src/domain/audit/policy.rs::is_journaled` (`RequestKind::Consultation`).
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS` (`audit: None` pour `/hello`, `/me`, `/operations/{id}`).

## Vérification
- `domain::audit::policy::tests::what_is_journaled_follows_the_rules_of_the_spec`.
- `crates/hearth-agent/tests/audit_https.rs::an_account_creation_produces_its_entry_with_who_where_and_what` (relire le journal n'ajoute rien).

## Cas limites
- Une consultation qui échoue (par exemple un paramètre invalide) n'est pas non plus consignée.

## Règles liées
- BR-AUDIT-003, BR-AUDIT-021.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
