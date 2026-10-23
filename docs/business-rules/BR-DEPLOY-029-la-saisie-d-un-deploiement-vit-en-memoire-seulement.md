---
id: BR-DEPLOY-029
domaine: DEPLOY
titre: La saisie d'un déploiement vit en mémoire seulement : jamais dans l'adresse, sur le disque ou dans un instantané ; fermer un formulaire rempli demande de confirmer
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-029, amendée le 2026-10-13), HRT-69
maj: 2026-10-24
---

# BR-DEPLOY-029 — La saisie d'un déploiement vit en mémoire seulement : jamais dans l'adresse, sur le disque ou dans un instantané ; fermer un formulaire rempli demande de confirmer

## Règle
Le formulaire « Déployer un service » est toujours vide à l'ouverture. Sa saisie (nom, image, variables dont les valeurs, ports, commande, ressources) vit dans `stores/deployForm.ts`, un magasin **sans persistance** dont les brouillons ne font pas partie de l'état sérialisable : ni l'adresse, ni le stockage du navigateur, ni un instantané ne la reçoivent, et fermer l'application l'efface. Les valeurs de variables et la commande restent en mémoire jusqu'à ce que l'issue du déploiement soit connue : un **échec sans service créé** et une **annulation** rouvrent le formulaire rempli ; une réussite, un service conservé, un résultat inconnu, « Continuer en arrière-plan » et l'abandon l'effacent aussitôt (chaque valeur est écrasée avant que les références ne soient lâchées). Fermer le formulaire (bouton « Fermer », Échap hors d'un champ, autre entrée du menu) quand au moins un champ est rempli demande « Abandonner votre saisie ? » (« Abandonner », « Continuer la saisie ») ; un formulaire vide se ferme sans question. Quitter la page efface la saisie.

## Application (code)
- `apps/desktop/src/stores/deployForm.ts::{of, wipe, wipeAll}`
- `apps/desktop/src/deploy/draft.ts::{isFilled, wipeDraft}`
- `apps/desktop/src/pages/DeployService.vue (onBeforeRouteLeave, abandon, requestClose)`
- `apps/desktop/src/composables/useDeployFlow.ts (watch finished, background, close, seeLogs)`

## Vérification
- `apps/desktop/src/stores/deployForm.test.ts`
- `apps/desktop/src/pages/DeployFlow.test.ts (abandonner sa saisie ; la saisie ne va jamais dans l'adresse)`

## Cas limites
- Écraser une chaîne n'efface pas la mémoire (JavaScript les garde immuables) : on lâche toute référence, c'est le mieux que le navigateur permette.
- « Continuer en arrière-plan » efface la saisie sans attendre l'issue (choix conservateur) : un échec ultérieur est annoncé par une notification, sans formulaire à rouvrir.

## Règles liées
BR-DEPLOY-006, BR-DEPLOY-014, BR-DEPLOY-037

## Historique
- 2026-10-24 — création (HRT-69, déployer une image, côté client).
