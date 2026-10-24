---
id: BR-FICHE-043
domaine: FICHE
titre: La recherche trouve un réglage par libellé, nom technique et phrase, sur tous les thèmes et tous les niveaux, sans tenir compte des majuscules ni des accents
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-043), HRT-109
maj: 2026-10-24
---

# BR-FICHE-043

## Règle
Le champ « Chercher un réglage » filtre sur tous les thèmes et tous les niveaux (même en vue « Courants »), par libellé, nom technique du jeu et phrase d'explication, sans tenir compte des majuscules ni des accents ; un thème qui contient un résultat s'ouvre ; le nombre de résultats est annoncé aux lecteurs d'écran. Le filtre « Modifiés » ne garde que les réglages différents de la valeur proposée. Aucun résultat : « Aucun réglage ne correspond. ». La touche « / » met le curseur dans le champ.

## Application (code)
- `apps/desktop/src/settings/filter.ts::{plain, matches, shown}`, `apps/desktop/src/components/organisms/GameSettingsList.vue`

## Vérification
- `apps/desktop/src/settings/settings.test.ts::chercher, filtrer, ranger (BR-FICHE-043, 045)`
- `apps/desktop/src/components/organisms/gameSettingsTab.test.ts::chercher et filtrer (BR-FICHE-043, 045)`

## Cas limites
- Un réglage « modifié » mais avancé reste caché en vue « Courants » ; le compteur de son thème le compte.

## Règles liées
BR-FICHE-041, BR-FICHE-045

## Historique
- 2026-10-24 — création (HRT-109).
