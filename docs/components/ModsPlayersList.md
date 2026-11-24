# ModsPlayersList

Organisme · `apps/desktop/src/components/organisms/ModsPlayersList.vue`

« Ce que vos joueurs doivent installer » (HRT-264, BR-MOD-029, maquette Joueurs) : les mods activés, « Aussi chez les joueurs » ou « Serveur seulement » d'après ce que le mod dit de lui-même (jamais « Obligatoire »), l'adresse du serveur et un seul bouton, « Copier la liste ». Aucun enregistrement en fichier. La mise en forme du texte copié est `services/modsCopy.ts`, partagée avec « Copier la liste » d'une sauvegarde (HRT-294).

- Props : `view`, `address`
- Événements : `back`
- Notes : écart à la maquette : pas de bouton « Enregistrer la liste en fichier texte » (décision de Voiky du 2026-11-16) ; la section « Un dossier prêt à copier chez le joueur » est gardée telle quelle (à confirmer). Tests `pages/ServicesModsApply.test.ts`, `services/modsCopy.test.ts`.

Un mod activé « à compléter » (dépendance absente) n'est ni listé ni copié : une alerte le dit avec le mod manquant.
