# InstallResources

Organisme · `apps/desktop/src/components/organisms/InstallResources.vue`

Les « Ressources » de l'installation (HRT-102, BR-FICHE-020 à 023, BR-RES-013) : `ResourceSliders` placés sur ce que la fiche recommande (jamais sur un chiffre sans preuve, jamais « sans limite »), le minimum de la fiche, la recommandation ramenée à la capacité du serveur, les avertissements chiffrés en direct (ce qui reste de mémoire, limite sous le minimum ou au-dessus de ce qui reste) et la place sur le disque (erreur sans case pour un besoin mesuré, avertissement pour le chiffre de l'éditeur). Les cases « Je comprends » sont dans la fenêtre « Vérifiez avant de lancer ».

- Props : `install` (`FicheInstallState`)
- Événements et slots : —
- Notes : Tests : `pages/InstallFromFiche.test.ts`.
