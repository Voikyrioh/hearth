---
id: BR-SVC-017
domaine: SVC
titre: Lien non « Connecté » : la liste garde les dernières données, grisées, avec leur âge
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-017), HRT-62
maj: 2026-10-13
---

# BR-SVC-017 — Lien non « Connecté » : la liste garde les dernières données, grisées, avec leur âge

## Règle
Quand le lien n'est pas « Connecté », la liste (et le détail, plus tard) gardent les dernières données connues, grisées, avec leur âge (`BR-RESIL-007`) ; elles se mettent à jour toutes seules au retour du lien. Les actions sont désactivées avec la raison (`BR-RESIL-008`). Un seul mécanisme date la page à la fois : lien coupé, c'est le gabarit du serveur (`StaleSurface`) ; lien bon et moteur non prêt ou liste non relisible, c'est la mention « Liste du moteur d'il y a {âge} » (`BR-SVC-018`).

L'âge d'une liste ne vient jamais de l'heure à laquelle l'interface traite un événement : il part de la date où l'agent a dit la liste à jour (`list_current_at`), ramenée à l'horloge du poste par l'heure de RÉCEPTION de la vue du moteur, que donne la liaison (`engine_received_at`, portée dans l'événement et sur le disque). Un état rejoué ou relu du disque garde donc sa vraie date ; un repère inconnu n'est pas inventé (aucun âge affiché).

## Application (code)
- `apps/desktop/src/services/view.ts::show`, `::listCurrentAt`
- `apps/desktop/src/stores/services.ts::apply`
- `apps/desktop/src/pages/Services.vue`
- Date de réception : `crates/hearth-link/src/domain/services.rs::engine_received_at`, `crates/hearth-link/src/domain/server.rs::LastKnown (services_at)`, `apps/desktop/src-tauri/src/services/book.rs::ServicesBook`.
- HORS DOMAIN pour l'affichage : porté par l'interface.

## Vérification
- `apps/desktop/src/services/services.test.ts` : « âge de la liste d'un moteur non prêt (I3, I4) ».
- `apps/desktop/src/stores/services.test.ts` : « le repère de réception vient de la liaison, jamais de l'heure du traitement (I4) » (état rejoué, instantané relu du disque, moteur arrêté depuis peu).
- `crates/hearth-link/src/domain/services.rs::tests::the_engine_view_is_dated_by_its_reception_and_a_repeated_view_keeps_its_date`
- `apps/desktop/src-tauri/tests/services_runtime.rs::the_event_the_window_receives_is_the_reference_file_field_by_field` (rejeu).

## Règles liées
- BR-ENGINE-015, BR-SVC-018, BR-RESIL-007, BR-RESIL-008

## Historique
- 2026-10-13 — création (HRT-62, section Services du client, lecture seule).
- 2026-10-13 — revue de la PR 79 (I3, I4) : l'âge part de `list_current_at` et le repère de réception vient de la liaison.
