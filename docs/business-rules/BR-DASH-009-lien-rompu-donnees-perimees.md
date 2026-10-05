---
id: BR-DASH-009
domaine: DASH
titre: Lien rompu : dernières valeurs grisées avec leur âge
statut: partielle
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-009), technique-socle §2, §3, §7, §10, HRT-06
maj: 2026-10-05
---

# BR-DASH-009 — Lien rompu : dernières valeurs grisées avec leur âge

## Règle
Côté agent : chaque échantillon porte l'instant `at` de la mesure (RFC 3339 UTC), ce qui permet au client de calculer l'âge (« Vu il y a 2 min »). L'affichage grisé et l'âge sont l'affaire du client (HRT-07, HRT-11).

## Application (code)
- `crates/hearth-proto/src/api/metrics.rs::Sample` (champ `at`).

## Vérification
- `hearth_proto::api::metrics::tests`

## Cas limites
- `at` vient de l'horloge de l'agent, pas de celle du client.

## Règles liées
- BR-DASH-011

## Interface
- `pages/Dashboard.vue` enveloppe la grille dans `StaleSurface` (désaturée, opacité .62, « Vu il y a … ») quand le lien n'est pas « Connecté » ; le store garde l'identité, le dernier échantillon et l'historique. Au démarrage hors ligne, `get_dashboard` rend la dernière vue du disque. Tests : `pages/Dashboard.test.ts`, `src-tauri/tests/dashboard_runtime.rs`, `e2e/dashboard.spec.ts`.

## Historique
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
- 2026-10-05 — interface du tableau de bord (HRT-11, session 2026-10-04-hearth-creation).
