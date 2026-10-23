---
id: BR-DEPLOY-033
domaine: DEPLOY
titre: Lire le formulaire, vérifier les ports et les dossiers sont des consultations, non journalisées, qui n'écrivent rien
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-033), HRT-68
maj: 2026-10-24
---

# BR-DEPLOY-033 — Lire le formulaire, vérifier les ports et les dossiers sont des consultations, non journalisées, qui n'écrivent rien

## Règle
`POST /deployments/check` est un `POST` qui ne modifie rien (`READ_ONLY_POSTS`) : pas de suivi, pas d'action de journal, aucune écriture. Elle ne reçoit jamais un secret (ni variables, ni commande). Elle ne lit jamais le contenu d'un fichier.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/deployments.rs::check`
- `crates/hearth-agent/src/entrypoint/http/mod.rs (`READ_ONLY_POSTS`, classe `ServiceCheck`)`
- `apps/desktop/src/deploy/form.ts::toCheckInput` (client : la consultation ne porte jamais un secret)

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::the_check_reports_each_field_writes_nothing_and_is_not_journaled`
- `apps/desktop/src/deploy/deploy.test.ts (la consultation ne porte JAMAIS un secret)`

## Cas limites
- Aucun cas particulier connu.

## Règles liées
BR-DEPLOY-032

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
- 2026-10-24 — côté client (HRT-69, déployer une image).
