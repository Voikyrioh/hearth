---
id: FIX-01M5PYVP4WVBS6H4Q6M1QW4DQG
titre: Le simulateur donnait deux machines : 16 cœurs et 64 Go au tableau de bord, 4 cœurs et 16 Go aux curseurs et à la fiche (HRT-69)
date_découverte: 2026-10-24
date_correction: 2026-10-24
---

# FIX-01M5PYVP4WVBS6H4Q6M1QW4DQG : Le simulateur donnait deux machines : 16 cœurs et 64 Go au tableau de bord, 4 cœurs et 16 Go aux curseurs et à la fiche (HRT-69)

## Symptôme
Le tableau de bord de « forge » dit 16 cœurs, 64 Go, 590 Go libres ; les curseurs de ressources s'arrêtent à 4 cœurs et 16 Go, la fiche d'un jeu dit « Ce serveur : 4 cœurs, 16 Go », l'étape Ressources « 200 Go libres ». Aucune phrase n'explique l'écart.

## Cause root
Le pont simulé avait deux sources : la machine simulée (tableau de bord) et des valeurs par défaut écrites en dur dans `SimulatedDeploys` (4 cœurs, 16 Gio, 200 Gio) pour la capacité et le disque des images. Sur le vrai serveur les deux viennent de la même machine.

## Reproduction
Dans le navigateur de développement : comparer le tableau de bord et le formulaire de déploiement. Test : `src/link/simulated-deploy.test.ts` (« la machine simulée donne les mêmes chiffres que le tableau de bord »), `e2e/ux-2026-10-24.spec.ts` (constat 3).

## Impacté
`link/simulated-deploy.ts`, `link/simulated-machine.ts`, `link/simulated.ts` (simulateur seulement, jamais livré).

## Workaround
Aucun.

## Correction
`SimulatedMachine.hostOf` donne processeurs, mémoire et espace libre du premier disque ; `SimulatedDeploys` les lit (réserve : `reserveOf`, la formule de `hearth_proto`). Les tests de pages posent une petite machine (4 cœurs, 16 Go) dans `test/deploy.ts`.

## Références
- Revue UX de Nora du 24/10, constat 3 ; ticket : HRT-69
- Code : `apps/desktop/src/link/simulated-deploy.ts` (marqueur `FIX:01M5PYVP4WVBS6H4Q6M1QW4DQG`)
