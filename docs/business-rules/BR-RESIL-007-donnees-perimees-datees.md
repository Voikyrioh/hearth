---
id: BR-RESIL-007
domaine: RESIL
titre: Hors « Connecté », les dernières données restent affichées, désaturées et datées
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-007), HRT-09
maj: 2026-10-05
---

# BR-RESIL-007 — Hors « Connecté », les dernières données restent affichées, désaturées et datées

## Règle
Tant que le lien n'est pas « Connecté » (reconnexion, hors ligne, session expirée, accès révoqué), les vues gardent les dernières données connues, désaturées (`grayscale(.85)`, opacité .62), avec leur âge : « Vu il y a 12 s » / « Vu il y a 2 min » / « Vu il y a 3 h » / « Vu il y a 2 j », mis à jour en direct. Un seul mécanisme : `StaleSurface`. Les données ne sont jamais retirées. La bibliothèque conserve la dernière vue connue et sa date (`SnapshotStore`, `LinkManager::last_known`) ; le marquage visuel est de l'interface.

## Application (code)
- Bibliothèque :
  - Données fournies par `crates/hearth-link/src/domain/state.rs::LinkMachine::status` et par les événements du `LinkManager` (`SnapshotStore`, `LinkManager::last_known`).
- Interface :
  - `apps/desktop/src/components/molecules/StaleSurface.vue` (désaturation, `data-stale`) et `StaleStamp.vue` (âge) ; `apps/desktop/src/composables/format.ts::formatSeen` ; horloge partagée `composables/useNow.ts`.
  - Utilisé par `ComingSoonPanel` (Dashboard, Accounts, Audit), puis par les vraies cartes des tickets suivants.
- Interface : c'est le GABARIT du serveur (`ServerLayout`) qui enveloppe toute page de `StaleSurface` (désaturation, opacité `--opacity-stale` = 0,62, âge `StaleStamp` « Vu il y a X min » actualisé par `useNow`) quand le lien n'est pas « Connecté » : une page présente ou à venir ne peut pas l'oublier, et ne l'enveloppe pas elle-même. Test : `shell.test.ts::dims and dates EVERY page of a server from the layout` (parcourt les routes du routeur).

## Vérification
- Interface : `molecules.test.ts::StaleStamp`, `::StaleSurface`, `format.test.ts::formatSeen`, `shell.test.ts::goes connected -> reconnecting -> offline -> back`, `e2e/shell.spec.ts`.
- Bibliothèque : tests de la dernière vue conservée (voir BR-RESIL-017).
- Interface : `apps/desktop/e2e/offline.spec.ts` (opacité mesurée entre 0,5 et 0,7, âge affiché, pages « Comptes » et « Journal d'activité »).

## Cas limites
- Horloge reculée : l'âge ne devient jamais négatif (« Vu il y a 0 s »).
- Aucun contact connu : « Jamais vu ».
- Un seul minuteur d'une seconde pour toute l'interface, arrêté quand plus rien ne l'utilise (BR-RESIL-017).

## Règles liées
- BR-RESIL-001, BR-RESIL-002 à BR-RESIL-005, BR-RESIL-008, BR-DASH-009.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 — création de la partie interface (HRT-09, session 2026-10-04-hearth-creation). Portée par l'interface ; le calcul des états est dans `hearth-link` (ADR-0007).
- 2026-10-05 — fiches HRT-07 et HRT-09 réunies (fusion de main dans feat/HRT-07-link).
- 2026-10-05 : vérifications de bout en bout (HRT-12).
- 2026-10-05 : garantie structurelle : `StaleSurface` dans le gabarit du serveur, plus par convention de page (revue HRT-12).
