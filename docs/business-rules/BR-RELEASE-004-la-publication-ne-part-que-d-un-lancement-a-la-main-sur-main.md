---
id: BR-RELEASE-004
domaine: RELEASE
titre: La publication ne part que d'un lancement à la main sur main
statut: active
invariant: true
source: HRT-172, contexts/hearth/conceptions/2026-10-16-fonctionnelle-publier-une-version.md (BR-RELEASE-004), 2026-10-16-technique-publier-une-version.md (7.2, 10)
maj: 2026-10-17
---

# BR-RELEASE-004 : Lancement à la main sur main seulement

## Règle
La publication d'une version ne part que d'un lancement à la main (`workflow_dispatch`) de la tête de `main` ; aucun autre événement (PR, push, étiquette, planification) ne la déclenche. Première tranche (HRT-172) : le premier contrôle de `cargo xtask release-preflight` compare `HEARTH_REF` (le `github.ref` du lancement) à `refs/heads/main`. Toute autre valeur (autre branche, étiquette, `refs/pull/…`, `main` sans préfixe, casse différente, vide) est refusée avant toute autre lecture, sans appel réseau, avec le message exact « La publication ne part que de main. ».

Deuxième tranche (HRT-175) : le flux `.github/workflows/publish.yml` n'a que `on: workflow_dispatch` (entrées `essai`, cochée par défaut, et `notes`, obligatoire) ; ni push, ni PR, ni étiquette, ni planification. La restriction de l'environnement `release` à la branche `main` est un réglage GitHub du propriétaire (runbook).

## Application (code)
- `xtask/src/release_preflight.rs::check_ref` : compare la référence à `refs/heads/main`.
- `xtask/src/release_preflight.rs::preflight` : appelle `check_ref` en premier (hors `--versions-only`, qui ne concerne que les numéros et les clés).
- `.github/workflows/publish.yml` (`on:`), contrôlé par `xtask/src/workflow_check.rs::trigger_violations`.

## Vérification
- `xtask/src/release_preflight.rs` : `only_the_head_of_main_is_published`, `every_refusal_is_the_message_of_the_spec_and_no_network_is_touched_before_the_last_checks`, `the_checks_run_in_the_order_of_the_conception`, `versions_only_checks_numbers_and_keys_without_the_network_or_the_variables`.
- `xtask/src/workflow_check.rs` : `publish_has_only_the_manual_trigger`.

## Cas limites
- Lancement depuis une autre branche : refus, aucun appel à `git`, `curl` ni à l'API.
- `--versions-only` (CI de chaque PR) ne contrôle pas la branche : une PR n'est pas sur `main`.

## Limites connues
- La barrière réelle contre un flux modifié sur une autre branche est la règle de branche de l'environnement `release` (conception 8.1), pas ce contrôle : un flux modifié pourrait ne pas appeler `release-preflight`.

## Règles liées
- BR-RELEASE-006 et 008 (autres contrôles préalables) ; BR-RELEASE-005 (aucune clé avant l'approbation).

## Historique
- 2026-10-16 : création (HRT-172).
- 2026-10-17 : déclencheur du flux et son test (HRT-175).
