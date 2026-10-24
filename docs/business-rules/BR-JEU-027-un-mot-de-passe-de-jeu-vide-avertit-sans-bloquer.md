---
id: BR-JEU-027
domaine: JEU
titre: Un mot de passe de jeu laissé vide avertit sans bloquer
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-JEU-027), HRT-102
maj: 2026-10-24
---

# BR-JEU-027

## Règle
Règle portée par le client (HRT-102). Quand une question de genre secret et de nature « mot de passe du jeu » est laissée vide, un encart prévient : « Aucun mot de passe : toute personne qui peut joindre ce port peut rejoindre la partie. ». Rien n'est bloqué, y compris avec « Installer avec les valeurs proposées ».

## Application (code)
- `apps/desktop/src/components/molecules/QuestionField.vue`

## Vérification
- `apps/desktop/src/pages/InstallFromFiche.test.ts::avertit d'un mot de passe laissé vide sans bloquer`

## Cas limites
- Le mot de passe n'est jamais écrit dans le journal d'activité ni affiché ailleurs que dans son champ masqué.

## Règles liées
BR-FICHE-034, BR-ACCT-006

## Historique
- 2026-10-24 — création (HRT-102).
