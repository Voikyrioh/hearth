# SettingThemeCard

Organisme · `apps/desktop/src/components/organisms/SettingThemeCard.vue`

Une carte de thème repliable (HRT-109, BR-FICHE-041) : un `details` natif, titre `h2`, compteur « {n} réglage(s), {m} modifié(s) » et « Remettre ce thème aux valeurs proposées ». Ouverte par défaut quand le thème contient un réglage essentiel ou modifié ; une recherche l'ouvre.

- Props : `label`, `total`, `modified`, `open`, `forceOpen`, `canReset`, `disabled`
- Événements et slots : `reset` ; slot par défaut (les lignes)
- Notes : Tests : `components/organisms/gameSettingsTab.test.ts`.
