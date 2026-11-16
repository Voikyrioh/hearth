---
id: BR-FICHE-066
domaine: FICHE
titre: La fin d'une installation depuis une fiche donne l'adresse à envoyer aux joueurs et les ports à ouvrir, sans rien promettre
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-ajouter-un-serveur-de-jeu.md (BR-FICHE-066), HRT-247
maj: 2026-11-16
---

# BR-FICHE-066 — Une installation réussie finit sur l'adresse à donner aux joueurs

## Règle
Quand une installation depuis une fiche réussit (l'opération est `succeeded` sans « le jeu s'arrête aussitôt »), la page ne ramène pas à la liste : elle montre l'écran de fin. Il dit « Le serveur de jeu « {nom} » est en marche. », ou « démarre. » avec « Le premier démarrage peut prendre jusqu'à {d} minutes. » quand la fiche déclare un premier démarrage long (la phase du jeu n'est pas encore lue, HRT-197) ; le monde créé est nommé quand la fiche en nomme un. Les adresses sont celles de la carte d'adresse du détail (réseau maison et internet, « Copier », BR-JEU-043), lues sur l'agent ; les ports à ouvrir sont ceux que l'agent publie (port, protocole, rôle), la console à distance est dite non publiée quand la fiche en déclare une. Hearth ne promet jamais qu'une adresse est joignable. Une lecture des adresses qui échoue se dit avec « Réessayer » sans cacher le reste. Les étapes de l'installation sont repliées. Une installation mise en arrière-plan, échouée ou interrompue n'a pas d'écran de fin : son issue reste celle de la progression. Le formulaire libre garde son issue (notification, liste des services).

## Application (code)
- `apps/desktop/src/components/organisms/InstallDone.vue`
- `apps/desktop/src/composables/useDeployFlow.ts::useDeployFlow` (option `onSucceeded`)
- `apps/desktop/src/pages/InstallFromFiche.vue`

## Vérification
- `apps/desktop/src/components/organisms/installDone.test.ts`
- `apps/desktop/src/pages/InstallFromFiche.test.ts::installe avec le mot de passe de confirmation à chaque fois, suit l'opération et efface la saisie à l'issue`
- `apps/desktop/e2e/ajouter-un-serveur-de-jeu.spec.ts`

## Cas limites
- La saisie (mot de passe de jeu compris) est effacée au lancement : l'écran de fin ne montre aucune valeur de réglage.
- Le nom du monde créé est retenu quand la vérification s'affiche ; un monde repris ne se dit pas comme créé.

## Règles liées
- BR-JEU-043, BR-FICHE-025, BR-FICHE-034

## Historique
- 2026-11-16 — création (HRT-247).
