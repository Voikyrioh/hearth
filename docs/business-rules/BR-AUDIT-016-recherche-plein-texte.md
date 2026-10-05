---
id: BR-AUDIT-016
domaine: AUDIT
titre: La recherche plein texte porte sur tous les champs visibles
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-016), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-016 — La recherche plein texte porte sur tous les champs visibles

## Règle
`q` cherche dans le compte, l'adresse IP, le nom du poste, le libellé de l'action, la cible et la raison, sans tenir compte de la casse ni des accents. Plusieurs mots : ET (tous doivent figurer). Chaque mot est cherché en début de mot (`10.0.0` trouve `10.0.0.7`). **La saisie est du texte, jamais une requête** : chaque mot devient une chaîne entre guillemets (guillemets doublés) pour le moteur, si bien que `"`, `*`, `-`, `:`, `(`, `OR`, `NEAR`… sont des mots ordinaires et ne peuvent ni changer le sens de la recherche ni la faire échouer. Un caractère de contrôle sépare les mots. Une saisie sans lettre ni chiffre n'a pas d'effet.

## Application (code)
- `crates/hearth-agent/src/domain/audit/filter.rs::SearchQuery::parse`.
- `crates/hearth-agent/migrations/0003_audit_events.sql` (table `audit_fts`, FTS5 en contenu externe, tenue à jour par déclencheurs).
- `crates/hearth-agent/src/infrastructure/sqlite/audit_repo.rs::SqliteAuditRepo::search`.

## Vérification
- `domain::audit::filter::tests` (échappement, bornes).
- `crates/hearth-agent/tests/audit_repo.rs::the_search_finds_every_visible_field_regardless_of_case_and_accents`.
- `crates/hearth-agent/tests/audit_repo.rs::special_characters_in_the_search_are_text_never_a_query`.
- `crates/hearth-agent/tests/audit_repo.rs::the_purge_removes_by_age_and_then_by_count_and_keeps_the_search_in_step`.

## Cas limites
- Au plus 8 mots de 64 caractères pris en compte.
- Le libellé cherché est celui écrit avec l'entrée (figé) ; le code de l'action n'est pas indexé.

## Règles liées
- BR-AUDIT-015.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
