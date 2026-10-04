---
id: BR-AUDIT-010
domaine: AUDIT
titre: Les nouvelles entrées arrivent en direct
statut: serveur prêt, interface à venir (HRT-14)
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-010), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-010 — Les nouvelles entrées arrivent en direct

## Règle
Côté interface : en haut du journal, une nouvelle entrée apparaît sans rechargement ; défilé vers le bas, un bouton « N nouvelles entrées » apparaît. Côté agent : chaque entrée écrite est diffusée sur un canal interne une fois sa transaction validée ; le flux temps réel (HRT-06, sujet `audit`, administrateurs seulement) la transmettra. Une page se relit par curseur (`before`, `next_before`).

## Application (code)
- `crates/hearth-agent/src/application/ports/audit_sink.rs::AuditFeed`.
- `crates/hearth-agent/src/infrastructure/audit_feed.rs::BroadcastAuditFeed`.
- `crates/hearth-agent/src/application/audit.rs::AuditService::subscribe` (réservé aux administrateurs).
- `hearth-proto::api::audit::AuditEventItem` (même type dans le message `audit { event }`).

## Vérification
- `infrastructure::audit_feed::tests`.
- `crates/hearth-agent/tests/audit_use_cases.rs::an_entry_is_published_once_the_action_is_committed_and_never_before_or_for_a_failure`.

## Cas limites
- Aucune route WebSocket dans ce ticket : le port est prêt, le sujet `audit` est à brancher par le flux (HRT-06).

## Règles liées
- BR-AUDIT-011, BR-AUDIT-020.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
