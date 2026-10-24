---
id: BR-DEPLOY-014
domaine: DEPLOY
titre: La commande facultative remplace celle de l'image ; elle est découpée en arguments par les espaces, les guillemets regroupent, et elle n'est jamais passée à un interpréteur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-014), HRT-68
maj: 2026-10-24
---

# BR-DEPLOY-014 — La commande facultative remplace celle de l'image ; elle est découpée en arguments par les espaces, les guillemets regroupent, et elle n'est jamais passée à un interpréteur

## Règle
Redirections, enchaînements, variables et échappements n'ont aucun effet : ce sont des caractères comme les autres. 4 096 caractères au plus, guillemets fermés, 64 arguments au plus. La commande est un secret (`Secret`) : scellée dans la description, jamais tracée ni signée (seul « il y a une commande » l'est).

## Application (code)
- `crates/hearth-agent/src/domain/services/command.rs::split_command`
- `apps/desktop/src/deploy/rules.ts::{splitCommand, commandIssue}` (client : mêmes vecteurs que l'agent)

## Vérification
- `crates/hearth-agent/src/domain/services/command.rs::tests`
- `crates/hearth-agent/tests/service_deploy_https.rs::variables_ports_a_command_and_a_data_folder_reach_the_container_from_the_description`
- `apps/desktop/src/deploy/deploy.test.ts (la commande de lancement)`

## Cas limites
- Une commande vide ou faite d'espaces vaut « celle de l'image ».

## Règles liées
BR-DEPLOY-006

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
- 2026-10-24 — côté client (HRT-69, déployer une image).
