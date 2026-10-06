---
id: BR-AUDIT-009
domaine: AUDIT
titre: Le journal ne se modifie ni ne se vide depuis le client
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-009), HRT-05
maj: 2026-10-06
---

# BR-AUDIT-009 — Le journal ne se modifie ni ne se vide depuis le client

## Règle
Aucune route ne modifie ni ne supprime une entrée ; le seul chemin de suppression est le nettoyage de conservation (BR-AUDIT-008). La base refuse en plus toute modification d'une entrée écrite (déclencheur `audit_events_no_update`). Les ports de lecture n'ont aucune méthode d'écriture.

## Application (code)
- `crates/hearth-agent/migrations/0003_audit_events.sql` (déclencheur).
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS` (seulement `GET /audit` et `GET /audit/export`).
- `crates/hearth-agent/src/application/ports/audit_repo.rs` (`AuditRepo` : lecture ; `AuditTx` : écrire, compter, purger).
- `apps/desktop/src-tauri/build.rs::COMMANDS` (seulement `read_audit` et `export_audit` : des lectures, aucune écriture)

## Interface
Le client n'expose aucune commande qui modifie ou supprime une entrée : deux lectures typées (`GET` construit côté Rust), rien d'autre ; aucun bouton de suppression.

## Vérification
- `crates/hearth-agent/tests/audit_repo.rs::an_entry_cannot_be_modified_in_place`.
- `apps/desktop/src-tauri/tests/capabilities.rs` (aucune commande générique, aucun paramètre libre).

## Cas limites
- Quelqu'un qui a la main sur la machine et sur le fichier `hearth.db` peut toujours le modifier : le journal protège contre le client, pas contre le système hôte.

## Règles liées
- BR-AUDIT-008.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » et pointeurs du client (HRT-14, session 2026-10-04-hearth-creation).
