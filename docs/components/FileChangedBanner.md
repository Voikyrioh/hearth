# FileChangedBanner

Organisme · `apps/desktop/src/components/organisms/FileChangedBanner.vue`

« Le fichier a changé depuis la dernière fois que Hearth l'a écrit » (HRT-109, BR-FICHE-048) : un tableau Réglage · Valeur de Hearth · Valeur du fichier et, pour chaque réglage, « Garder la valeur du fichier » (par défaut) ou « Remettre la mienne ». Si le réglage n'accepte pas la valeur du fichier, seule « Remettre la mienne » reste. Jamais un secret.

- Props : `drifts`, `models`, `choices`, `disabled`
- Événements et slots : `choose(id, choix)`
- Notes : Tests : `components/organisms/gameSettingsTab.test.ts`.
