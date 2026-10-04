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
La tentative refusée qui déclenche une attente (BR-CONN-006 : 5 échecs par couple identifiant + adresse ; BR-CONN-007 : 20 échecs en 10 minutes par adresse) écrit deux entrées : la connexion refusée, puis l'action « Blocage temporaire », toutes deux avec le compte visé **si l'identifiant saisi correspond à un compte existant** (sinon sans compte, BR-AUDIT-005 et 006) : (résultat refusé, raison « trop de tentatives, attente de N s »). Les seuils « à définir en conception technique » sont ceux de BR-CONN-006 et 007.

## Regroupement des refus et échecs répétés
Les refus faute de droits et les échecs écrits par la couche d'accès (hors transaction) sont regroupés quand ils sont **identiques** (même compte, même origine, même action, même résultat) : le premier est écrit, les suivants de la fenêtre de 60 secondes sont comptés sans être écrits, **une** entrée de synthèse (la dernière occurrence, `repeat_count` = le nombre d'autres fois, raison « … (N autres fois en 1 min) ») est écrite à la fin de la fenêtre (tâche toutes les 15 s, ou dès qu'un événement identique suit). Un compte lecture seule qui boucle sur `GET /audit` écrit donc deux entrées par minute au plus, pas une par requête. Les groupes suivis sont plafonnés à 1 024 ; au-delà, un événement nouveau est écrit tel quel. Les synthèses en attente à l'arrêt de l'agent sont perdues. La cible et la raison ne font pas partie de l'identité : deux refus du même compte vers deux routes d'administration à moins d'une minute n'écrivent qu'une entrée.

## Application (code)
- `crates/hearth-agent/src/domain/audit/repeat.rs::RepeatFilter` ; `application/audit.rs::AuditRecorder::{record, flush}` ; `entrypoint/tasks.rs::spawn_audit_flush`.
- `crates/hearth-agent/src/application/sessions.rs::SessionService::login_in_turn`.
- `crates/hearth-agent/src/domain/audit/action.rs::AuditAction::LoginLocked`.
- `crates/hearth-agent/src/domain/audit/event.rs::Reason::TooManyAttempts`.

## Vérification
- `domain::audit::repeat::tests` ; `crates/hearth-agent/tests/audit_use_cases.rs::a_thousand_identical_refusals_make_two_entries`, `::refusals_that_differ_are_not_grouped_and_a_later_one_brings_the_summary`.
- `crates/hearth-agent/tests/audit_use_cases.rs::the_attempt_that_locks_adds_a_lock_entry_and_attempts_during_the_wait_add_nothing`.

## Cas limites
- Les tentatives refusées **pendant** l'attente ne sont pas journalisées : une rafale contre un compte bloqué ne doit pas faire tourner les 50 000 entrées du journal et en chasser l'historique utile. Le blocage lui-même est écrit une fois.
- Une connexion refusée parce que la file de l'adresse est pleine (`429 TOO_MANY_ATTEMPTS`, `details.retry_after_s = 1`) n'est pas journalisée non plus (elle est tracée en `warn` : adresse et raison), pour la même raison.

## Règles liées
- BR-CONN-006, BR-CONN-007, BR-AUDIT-003.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
