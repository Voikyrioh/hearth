---
id: BR-AUDIT-007
domaine: AUDIT
titre: Le blocage temporaire d'un compte est journalisé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-007), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-007 — Le blocage temporaire d'un compte est journalisé

## Règle
La tentative refusée qui déclenche une attente (BR-CONN-006 : 5 échecs par couple identifiant + adresse ; BR-CONN-007 : 20 échecs en 10 minutes par adresse) écrit deux entrées : la connexion refusée, puis l'action « Blocage temporaire » (résultat refusé, raison « trop de tentatives, attente de N s »). Les seuils « à définir en conception technique » sont ceux de BR-CONN-006 et 007.

## Application (code)
- `crates/hearth-agent/src/application/sessions.rs::SessionService::login_in_turn`.
- `crates/hearth-agent/src/domain/audit/action.rs::AuditAction::LoginLocked`.
- `crates/hearth-agent/src/domain/audit/event.rs::Reason::TooManyAttempts`.

## Vérification
- `crates/hearth-agent/tests/audit_use_cases.rs::the_attempt_that_locks_adds_a_lock_entry_and_attempts_during_the_wait_add_nothing`.

## Cas limites
- Les tentatives refusées **pendant** l'attente ne sont pas journalisées : une rafale contre un compte bloqué ne doit pas faire tourner les 50 000 entrées du journal et en chasser l'historique utile. Le blocage lui-même est écrit une fois.
- Une connexion refusée parce que la file de l'adresse est pleine (`503 BUSY`) n'est pas journalisée non plus, pour la même raison.

## Règles liées
- BR-CONN-006, BR-CONN-007, BR-AUDIT-003.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
