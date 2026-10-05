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
Les refus faute de droits et les échecs écrits par la couche d'accès (hors transaction) sont regroupés par fenêtre de 60 secondes. **La clé ne contient que ce que le serveur connaît ou borne** : le compte authentifié (ou « anonyme »), l'action, le résultat, la cible (un compte existant ou le motif statique de la route) et la raison (énumération). **Ni le nom du poste ni l'adresse** : un client les fait varier à volonté (en-tête libre, plage IPv6) et ferait de chaque requête le premier d'un groupe. Le premier événement d'un groupe est écrit ; les suivants sont comptés ; **une** entrée de synthèse est écrite à la fin de la fenêtre (tâche toutes les 15 s, et à l'arrêt de l'agent pour tout ce qui est en attente) : elle reprend l'origine de la **dernière** occurrence, avec `repeat_count` et la raison « … (N autres fois en 1 min) ».

Conséquences : un compte lecture seule qui boucle sur `GET /audit` avec un nom de poste et une adresse différents à chaque requête écrit deux entrées (5 000 requêtes, 2 entrées) ; le même compte qui vise le mot de passe de marie, de paul puis de carl dans la minute laisse une trace pour chacun (la cible est dans la clé) ; deux refus du même compte vers deux routes différentes (actions différentes) restent distincts.

Garde-fou : au plus 1 024 groupes suivis. Avec cette clé un compte n'en produit qu'un nombre borné par les cibles et raisons possibles. Au débordement, les événements nouveaux ne sont plus écrits un par un : ils sont comptés dans **un groupe de débordement unique**, résumé par une entrée « activité trop variée, N événements regroupés ».

## Application (code)
- `crates/hearth-agent/src/domain/audit/repeat.rs::RepeatFilter` ; `application/audit.rs::AuditRecorder::{record, flush}` ; `entrypoint/tasks.rs::spawn_audit_flush`.
- `crates/hearth-agent/src/application/sessions.rs::SessionService::login_in_turn`.
- `crates/hearth-agent/src/domain/audit/action.rs::AuditAction::LoginLocked`.
- `crates/hearth-agent/src/domain/audit/event.rs::Reason::TooManyAttempts`.

## Vérification
- `domain::audit::repeat::tests` ; `crates/hearth-agent/tests/audit_use_cases.rs::a_thousand_identical_refusals_make_two_entries`, `::refusals_of_different_accounts_are_not_grouped_and_a_later_one_brings_the_summary`, `::five_thousand_refusals_of_one_account_with_changing_hosts_and_addresses_make_two_entries`, `::targeting_three_accounts_in_a_minute_leaves_a_trace_for_each`, `::anonymous_events_of_one_action_group_whatever_the_number_of_addresses`, `::stopping_writes_every_pending_summary_before_the_windows_end`.
- `crates/hearth-agent/tests/audit_use_cases.rs::the_attempt_that_locks_adds_a_lock_entry_and_attempts_during_the_wait_add_nothing`.

## Cas limites
- Les tentatives refusées **pendant** l'attente ne sont pas journalisées : une rafale contre un compte bloqué ne doit pas faire tourner les 50 000 entrées du journal et en chasser l'historique utile. Le blocage lui-même est écrit une fois.
- Une connexion refusée parce que la file de l'adresse est pleine (`429 TOO_MANY_ATTEMPTS`, `details.retry_after_s = 1`) n'est pas journalisée non plus (elle est tracée en `warn` : adresse et raison), pour la même raison.

## Règles liées
- BR-CONN-006, BR-CONN-007, BR-AUDIT-003.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
