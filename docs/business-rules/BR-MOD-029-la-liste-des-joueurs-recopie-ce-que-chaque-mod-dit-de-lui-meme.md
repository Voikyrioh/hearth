---
id: BR-MOD-029
domaine: MOD
titre: La liste des joueurs recopie ce que chaque mod dit de lui-même
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-23
---

# BR-MOD-029 : La liste des joueurs recopie ce que chaque mod dit de lui-même

## Règle
Après une application réussie, l'écran de fin ouvre « Ce que vos joueurs doivent installer » : les mods ACTIVÉS, chacun « Aussi chez les joueurs » ou « Serveur seulement » d'après l'indication d'environnement que le mod donne sur lui-même (`environment = server` : serveur seulement ; tout le reste, y compris un mod qui ne dit rien : chez les joueurs, le plus sûr). Jamais « Obligatoire » : Hearth ne le sait pas. Une seule action, « Copier la liste » : nom, version, moteur et version de Minecraft de chaque mod à installer chez les joueurs, l'adresse du serveur en tête quand elle est connue. Aucun enregistrement en fichier, aucun fichier pour le lanceur. La même mise en forme sert « Copier la liste » d'une sauvegarde (BR-MOD-035).

## Application (code)
- `apps/desktop/src/services/modsCopy.ts::{audienceOf, copyText}`
- `apps/desktop/src/components/organisms/ModsPlayersList.vue`

## Vérification
- `apps/desktop/src/services/modsCopy.test.ts`
- `apps/desktop/src/pages/ServicesModsApply.test.ts` (ce que vos joueurs doivent installer)

## Règles liées
- BR-MOD-020, BR-MOD-025, BR-MOD-035

## Historique
- 2026-11-23 : création (HRT-264).
