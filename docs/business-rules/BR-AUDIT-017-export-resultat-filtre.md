---
id: BR-AUDIT-017
domaine: AUDIT
titre: L'export porte sur le résultat filtré
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-017), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-017 — L'export porte sur le résultat filtré

## Règle
`GET /audit/export` accepte les mêmes filtres que `GET /audit` (le curseur et la taille de page n'y comptent pas) et rend un CSV lisible dans un tableur : UTF-8 avec marque d'ordre des octets, séparateur `;`, fins de ligne `\r\n`, en-tête « Date et heure;Compte;Origine;Action;Cible;Résultat;Raison », dates en UTC. **Injection de formule** : toute valeur qui commence par `=`, `+`, `-`, `@`, une tabulation ou un retour chariot est précédée d'une apostrophe. Plafonné aux 10 000 entrées les plus récentes du résultat ; au-delà, l'en-tête `X-Hearth-Export-Truncated: true` le dit.

## Application (code)
- `crates/hearth-agent/src/domain/audit/csv.rs::{render, field}`.
- `crates/hearth-agent/src/application/audit.rs::AuditService::export`.
- `crates/hearth-agent/src/entrypoint/http/audit.rs::export`.

## Vérification
- `domain::audit::csv::tests`.
- `crates/hearth-agent/tests/audit_use_cases.rs::the_export_is_the_filtered_result_as_a_spreadsheet_file`.
- `crates/hearth-agent/tests/audit_use_cases.rs::an_export_stops_at_its_cap_and_says_so`.
- `crates/hearth-agent/tests/audit_https.rs::the_export_is_a_spreadsheet_file_of_the_filtered_result`.

## Cas limites
- L'export n'est lui-même pas journalisé (consultation, BR-AUDIT-004) ; refusé faute de droits, il l'est.

## Règles liées
- BR-AUDIT-004, BR-AUDIT-015.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
