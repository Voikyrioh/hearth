# HMiddleText

Atome · `apps/desktop/src/components/atoms/HMiddleText.vue`

Texte long tronqué AU MILIEU : le début et les 12 derniers caractères (`tail`) restent visibles, le milieu devient « … ». Tronqué, il prend le focus clavier (un seul arrêt de tabulation) et le texte complet se lit au survol et au focus (`HTooltip`) et par `aria-label` ; il tient, c'est du texte simple. Sert aux chemins (points de montage) et aux noms de sondes.

- Props : `text`, `tail` (12 par défaut)
- Notes : se réduit dans son parent flexible (`min-width: 0`). FIX:01M4EPX88BTXFX1PX7E57WGK38. Tests : `e2e/hrt47.spec.ts`.
