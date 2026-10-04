---
id: BR-AUDIT-009
domaine: AUDIT
titre: Le journal ne se modifie ni ne se vide depuis le client
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-009), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-009 — Le journal ne se modifie ni ne se vide depuis le client

## Règle
Aucune route ne modifie ni ne supprime une entrée ; le seul chemin de suppression est le nettoyage de conservation (BR-AUDIT-008). La base refuse en plus toute modification d'une entrée écrite (déclencheur `audit_events_no_update`). Les ports de lecture n'ont aucune méthode d'écriture.

## Application (code)
- `crates/hearth-agent/migrations/0003_audit_events.sql` (déclencheur).
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS` (seulement `GET /audit` et `GET /audit/export`).
- `crates/hearth-agent/src/application/ports/audit_repo.rs` (`AuditRepo` : lecture ; `AuditTx` : écrire, compter, purger).

## Vérification
- `crates/hearth-agent/tests/audit_repo.rs::an_entry_cannot_be_modified_in_place`.

## Cas limites
- Quelqu'un qui a la main sur la machine et sur le fichier `hearth.db` peut toujours le modifier : le journal protège contre le client, pas contre le système hôte.

## Règles liées
- BR-AUDIT-008.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
