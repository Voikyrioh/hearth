---
id: BR-RELEASE-005
domaine: RELEASE
titre: Aucune clé n'est lue avant l'approbation du propriétaire
statut: active
invariant: true
source: HRT-175, contexts/hearth/conceptions/2026-10-16-fonctionnelle-publier-une-version.md (BR-RELEASE-005), 2026-10-16-technique-publier-une-version.md (6, 8, 10)
maj: 2026-10-17
---

# BR-RELEASE-005 : Aucune clé avant l'approbation

## Règle
Aucune clé (signature du client, signature de l'agent, dépôt au bucket, écriture de la release GitHub) n'est lue avant l'approbation du propriétaire ; sans approbation, rien n'est signé, déposé ni étiqueté. Dans `.github/workflows/publish.yml` :
- `preflight`, `build-client`, `build-agent` et `review` n'ont aucun secret ni environnement ; `review` (permissions `{}`) est le point de relecture des journaux de construction.
- `publish` dépend de `preflight`, des deux constructions et de `review`, et déclare `environment: release` (approbation obligatoire du propriétaire, branche `main` seule) : GitHub n'y démarre aucune étape avant l'approbation.
- Les six secrets de l'environnement (`HEARTH_CLIENT_SIGNING_KEY`, `HEARTH_CLIENT_SIGNING_KEY_PASSWORD`, `HEARTH_AGENT_SIGNING_KEY`, `HEARTH_AGENT_SIGNING_KEY_PASSWORD`, `HEARTH_RELEASES_ACCESS_KEY_ID`, `HEARTH_RELEASES_SECRET_ACCESS_KEY`) ne sont cités que dans l'`env:` de l'ÉTAPE qui s'en sert, dans `publish`, jamais au niveau du job ni dans un autre job.
- Chaque job déclare ses permissions (`permissions: {}` par défaut) ; seul `release-github` a `contents: write`, et aucun secret.
- Aucune entrée du formulaire n'est interpolée dans un `run:` (elles passent par `env:`), tout extrait vient de `github.sha` avec `persist-credentials: false`, et `xtask` est compilé dans `preflight` (sans secret) : le job à clés n'exécute aucun script des dépendances du dépôt.

## Application (code)
- `.github/workflows/publish.yml` (jobs `review`, `publish`, `release-github`).
- `xtask/src/workflow_check.rs::{secret_violations, permission_violations, write_violations, checkout_violations, input_violations, pin_violations}` : lisent le fichier comme du texte et rendent les violations.

## Vérification
- `xtask/src/workflow_check.rs` : `secrets_appear_only_in_steps_of_the_release_job`, `every_job_declares_its_permissions`, `only_release_github_can_write_contents`, `checkout_never_persists_credentials`, `every_checkout_uses_github_sha`, `inputs_are_never_interpolated_in_run`, `every_action_is_pinned_by_sha`. Chacun vérifie le vrai fichier ET qu'une copie abîmée est refusée.

## Cas limites
- L'essai à blanc demande aussi l'approbation et lit les clés de signature (pas celles du bucket) : voir BR-RELEASE-011.

## Limites connues
- La barrière réelle contre un flux modifié sur une autre branche (garde retirée comprise) est la règle de branche de l'environnement `release`, un réglage GitHub que le dépôt ne peut pas vérifier ; il est décrit dans le runbook.
- Les tests lisent le texte, pas le YAML interprété : un changement de forme du fichier les fait échouer, c'est voulu.

## Règles liées
- BR-RELEASE-004, BR-RELEASE-007, BR-RELEASE-009, BR-RELEASE-011

## Historique
- 2026-10-17 : création (HRT-175).
