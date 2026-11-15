---
id: BR-MONDE-021
domaine: MONDE
titre: L'aide « J'ai déjà un serveur de jeu » est ouverte à tous, sans nommer aucun jeu ni dossier
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-reprendre-un-monde-existant.md (BR-MONDE-021), contexts/hearth/conceptions/2026-10-13-design-deploiements-lots-suivants.md (7.8, 16.5), HRT-106
maj: 2026-11-15
---

# BR-MONDE-021 — L'aide « J'ai déjà un serveur de jeu » est ouverte à tous, sans nommer aucun jeu ni dossier

## Règle
La page `/servers/:id/catalogue/serveur-existant` est accessible à tous les profils depuis l'option « Reprendre un monde existant ». Elle donne le parcours en trois temps dans cet ordre (essayer, basculer, adopter en facultatif), ce que chaque chemin fait gagner et perdre, deux encarts (ce qui ne change jamais, quels dossiers faire ouvrir) et se termine par « Si votre jeu n'a pas de fiche dans le Catalogue, seule l'adoption est possible. ». Elle ne nomme aucun jeu, aucun dossier et n'a aucune adresse. Le bouton « Adopter un service existant » est désactivé avec la phrase du droit pour un compte sans le droit de gérer les services.

## Application (code)
- `apps/desktop/src/pages/ExistingServerHelp.vue`
- `apps/desktop/src/router/index.ts` (route `existing-server-help`)
- `apps/desktop/src/components/organisms/WorldCard.vue` (le lien sous le choix du monde)

## Vérification
- `apps/desktop/src/pages/InstallWorld.test.ts` (l'aide « J'ai déjà un serveur de jeu »)

## Cas limites
- Le lien depuis le Catalogue n'existe pas : le Catalogue ne porte aucune ligne vers une aide (test `Catalogue.test.ts`).
- La phrase de pied suit le critère d'acceptation du ticket HRT-106 ; la spec dit aussi « ou l'installer vous-même comme un service » (écart noté pour la passe de fin).

## Règles liées
- BR-MONDE-027, BR-ADOPT-029

## Historique
- 2026-11-15 — création (HRT-106).
