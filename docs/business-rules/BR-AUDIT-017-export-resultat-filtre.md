---
id: BR-AUDIT-017
domaine: AUDIT
titre: L'export porte sur le résultat filtré
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-017), HRT-05
maj: 2026-10-06
---

# BR-AUDIT-017 — L'export porte sur le résultat filtré

## Règle
`GET /audit/export` accepte les mêmes filtres que `GET /audit` (le curseur et la taille de page n'y comptent pas) et rend un CSV lisible dans un tableur : UTF-8 avec marque d'ordre des octets, séparateur `;`, fins de ligne `\r\n`, en-tête « Date et heure;Compte;Origine;Action;Cible;Résultat;Raison », dates en UTC. **Injection de formule** : toute valeur qui commence par `=`, `+`, `-`, `@`, une tabulation ou un retour chariot, **ou par des espaces (même insécables) puis l'un des quatre premiers**, est précédée d'une apostrophe (une seule implémentation : `hearth_proto::api::audit_csv::field`). Plafonné aux 10 000 entrées les plus récentes du résultat ; au-delà, l'en-tête `X-Hearth-Export-Truncated: true` le dit.

## Application (code)
- `crates/hearth-agent/src/domain/audit/csv.rs::{render, field}`.
- `crates/hearth-agent/src/application/audit.rs::AuditService::export`.
- `crates/hearth-agent/src/entrypoint/http/audit.rs::export`.
- `apps/desktop/src-tauri/src/audit.rs::{export, SaveDialog, NativeSaveDialog}`
- `crates/hearth-link/src/manager/audit.rs::LinkManager::audit_export`
- `crates/hearth-proto/src/api/audit_csv.rs::{field, render_items}` (la neutralisation des formules, UNE seule implémentation partagée avec l'agent)

## Interface
« Exporter » (désactivé hors connexion et sans entrée) exporte le résultat filtré APPLIQUÉ (jamais un brouillon) : la coquille lit les données, ouvre la boîte d'enregistrement du système (nom suggéré `journal-hearth.csv`, confirmation d'écrasement par le système) et écrit le fichier là où l'utilisateur l'a choisi, nulle part ailleurs ; annuler n'écrit rien. « Export terminé » ou « Impossible de générer l'export » ; fichier tronqué à 10 000 entrées : avertissement. CSV UTF-8 avec BOM, séparateur `;`, dates en UTC, formules neutralisées une seule fois. Le fichier est écrit de façon ATOMIQUE (fichier temporaire à côté, synchronisation, renommage) : un export interrompu ne laisse jamais un CSV tronqué, et un export précédent écrasé n'est remplacé qu'une fois le nouveau entier. En-tête, séparateur, fins de ligne, colonnes et libellés : UN seul rendu (`audit_csv::render`, partagé par l'agent et la liaison).

## Vérification
- `domain::audit::csv::tests`.
- `crates/hearth-agent/tests/audit_use_cases.rs::the_export_is_the_filtered_result_as_a_spreadsheet_file`.
- `crates/hearth-agent/tests/audit_use_cases.rs::an_export_stops_at_its_cap_and_says_so`.
- `crates/hearth-agent/tests/audit_https.rs::the_export_is_a_spreadsheet_file_of_the_filtered_result`.
- `apps/desktop/src-tauri/tests/audit_runtime.rs::the_export_goes_where_the_user_chose_and_nowhere_else`, `cancelling_the_save_dialog_writes_nothing_and_a_bad_destination_is_a_storage_failure`.
- `crates/hearth-link/tests/audit.rs::the_export_is_the_filtered_result_as_a_spreadsheet_file_with_formulas_neutralized`.
- `crates/hearth-proto/src/api/audit_csv.rs` (tests : valeurs piégées dans chaque colonne).

## Cas limites
- L'export n'est lui-même pas journalisé (consultation, BR-AUDIT-004) ; refusé faute de droits, il l'est.

## Règles liées
- BR-AUDIT-004, BR-AUDIT-015.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » et pointeurs du client (HRT-14, session 2026-10-04-hearth-creation).
