---
id: BR-AUDIT-014
domaine: AUDIT
titre: Les filtres appliqués restent visibles et se réinitialisent en un clic
statut: serveur prêt, interface à venir (HRT-14)
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-014), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-014 — Les filtres appliqués restent visibles et se réinitialisent en un clic

## Règle
Côté interface : filtres visibles, bouton « Effacer les filtres », rien n'est mémorisé à la fermeture. Côté agent : les filtres sont des paramètres d'une requête sans état ; sans paramètre, la requête rend la page la plus récente.

## Application (code)
- `crates/hearth-agent/src/domain/audit/filter.rs::{AuditFilter, RawFilter}`.
- `crates/hearth-agent/src/entrypoint/http/audit.rs::filter_of`.

## Vérification
- `domain::audit::filter::tests`.
- `entrypoint::http::audit::tests`.

## Cas limites
- Aucun cas limite propre à l'agent.

## Règles liées
- BR-AUDIT-015.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
