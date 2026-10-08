---
id: BR-AUDIT-018
domaine: AUDIT
titre: Aucun événement : message et bouton d'effacement
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-018), HRT-05
maj: 2026-10-06
---

# BR-AUDIT-018 — Aucun événement : message et bouton d'effacement

## Règle
Côté interface : message « Aucun événement ne correspond » avec un bouton pour effacer les filtres. Côté agent : un filtre sans résultat rend `200` avec `events: []` et sans `next_before`, jamais une erreur.

## Application (code)
- `crates/hearth-agent/src/application/audit.rs::AuditService::search`.
- `apps/desktop/src/pages/Audit.vue` (`EmptyState`)

## Interface
Sans filtre : « Aucune activité enregistrée pour l'instant » ; avec filtre ou recherche : « Aucun événement ne correspond » et le bouton « Effacer les filtres ». Pas de tableau dans les deux cas.

## Vérification
- `crates/hearth-agent/tests/audit_repo.rs::the_search_finds_every_visible_field_regardless_of_case_and_accents` (recherche sans résultat).
- `apps/desktop/src/pages/audit.test.ts` (« journal vide », « pas de bouton Appliquer… »).
- `apps/desktop/e2e/audit.spec.ts` (« journal vide », « filtres »).

## Cas limites
- Aucun cas limite propre à l'agent.

## Règles liées
- BR-AUDIT-014.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » et pointeurs du client (HRT-14, session 2026-10-04-hearth-creation).
