---
id: BR-AUDIT-010
domaine: AUDIT
titre: Les nouvelles entrées arrivent en direct
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-010), HRT-05
maj: 2026-10-06
---

# BR-AUDIT-010 — Les nouvelles entrées arrivent en direct

## Règle
Côté interface : en haut du journal, une nouvelle entrée apparaît sans rechargement ; défilé vers le bas, un bouton « N nouvelles entrées » apparaît. Côté agent : chaque entrée écrite est diffusée sur un canal interne une fois sa transaction validée ; le flux temps réel (`entrypoint/ws`, sujet `audit`, administrateurs seulement, rôle relu toutes les 5 s) la convertit en `AuditEventItem` et la transmet. Une page se relit par curseur (`before`, `next_before`).

## Application (code)
- `crates/hearth-agent/src/application/ports/audit_sink.rs::AuditFeed`.
- `crates/hearth-agent/src/infrastructure/audit_feed.rs::BroadcastAuditFeed`.
- `crates/hearth-agent/src/application/audit.rs::AuditService::subscribe` (réservé aux administrateurs).
- `hearth-proto::api::audit::AuditEventItem` (même type dans le message `audit { event }`).
- `apps/desktop/src-tauri/src/link.rs::LinkRuntime::relay` (`Event::Audit` vers `link://audit`) et `LinkRuntime::open` (abonnement au sujet `audit`)
- `apps/desktop/src/stores/audit.ts::useAuditStore` (`ingest`, `showPending`, `setAtTop`, `catchUp`)
- `apps/desktop/src/components/organisms/AuditTable.vue` (détection du haut de liste)

## Interface
En haut de la liste, l'entrée arrive en tête sans rechargement ; défilée vers le bas, la liste ne bouge pas et le bouton « N nouvelles entrées » compte les entrées retenues (500 au plus ; au-delà on garde les plus anciennes et on relit la tête au clic). Un clic remonte et les montre ; revenir en haut à la main les montre aussi. L'ordre est stable (jamais de réordonnancement sous les yeux), aucun doublon (par identifiant), annonce aux lecteurs d'écran en zone polie, au plus une fois toutes les 2 s.

## Vérification
- `infrastructure::audit_feed::tests`.
- `crates/hearth-agent/tests/audit_use_cases.rs::an_entry_is_published_once_the_action_is_committed_and_never_before_or_for_a_failure`.
- `apps/desktop/src-tauri/tests/audit_runtime.rs::entries_written_by_the_agent_reach_the_window_as_link_audit_events`.
- `crates/hearth-link/tests/audit.rs::new_entries_arrive_live_exactly_once_for_an_administrator`.
- `apps/desktop/src/stores/audit.test.ts` (« direct »).
- `apps/desktop/e2e/audit.spec.ts` (« direct : en haut l'entrée apparaît… »).

## Cas limites
- Le sujet `audit` du flux est servi par le port `AuditFeed` (un seul port, événement du domaine) : test de bout en bout `tests/stream_https.rs::the_audit_topic_is_for_administrators_and_carries_the_account_creation`, et `::an_administrator_demoted_during_the_stream_loses_the_audit_topic`.

## Règles liées
- BR-AUDIT-011, BR-AUDIT-020.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » et pointeurs du client (HRT-14, session 2026-10-04-hearth-creation).
