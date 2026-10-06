---
id: BR-AUDIT-014
domaine: AUDIT
titre: Les filtres appliqués restent visibles et se réinitialisent en un clic
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-014), HRT-05
maj: 2026-10-06
---

# BR-AUDIT-014 — Les filtres appliqués restent visibles et se réinitialisent en un clic

## Règle
Côté interface : filtres visibles, bouton « Effacer les filtres », rien n'est mémorisé à la fermeture. Côté agent : les filtres sont des paramètres d'une requête sans état ; sans paramètre, la requête rend la page la plus récente.

## Application (code)
- `crates/hearth-agent/src/domain/audit/filter.rs::{AuditFilter, RawFilter}`.
- `crates/hearth-agent/src/entrypoint/http/audit.rs::filter_of`.
- `apps/desktop/src/audit/filters.ts` (brouillon, `resolveFilter`, `sameDraft`)
- `apps/desktop/src/stores/audit.ts::useAuditStore::{apply, clearFilters}`
- `apps/desktop/src/components/organisms/AuditFilters.vue`

## Interface
Les filtres sont édités dans la carte « Filtres » et ne s'appliquent qu'au clic sur « Appliquer les filtres » (actif seulement quand le brouillon diffère de ce qui est appliqué, avec un point d'attente) ou Entrée dans la recherche ; « Effacer les filtres » (visible dès qu'un filtre existe) remet tout à zéro et relit le journal complet ; un échec de lecture garde les filtres et la liste précédents. Rien n'est mémorisé à la fermeture. Un filtre actif s'applique aussi aux entrées reçues en direct : l'agent décide (relecture de la tête), jamais une recopie locale de ses règles.

## Vérification
- `domain::audit::filter::tests`.
- `entrypoint::http::audit::tests`.
- `apps/desktop/src/stores/audit.test.ts` (« filtres appliqués au direct »).
- `apps/desktop/src/pages/audit.test.ts` et `apps/desktop/e2e/audit.spec.ts` (filtres, effacer, direct filtré).

## Cas limites
- Aucun cas limite propre à l'agent.

## Règles liées
- BR-AUDIT-015.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » et pointeurs du client (HRT-14, session 2026-10-04-hearth-creation).
