# HMiddleText

Atome · `apps/desktop/src/components/atoms/HMiddleText.vue`

Texte long tronqué AU MILIEU : le début et les 12 derniers caractères (`tail`) restent visibles, le milieu devient « … ». Tronqué, il prend le focus clavier (un seul arrêt de tabulation) et le texte complet se lit au survol et au focus (`HTooltip`) ; il tient, c'est du texte simple. Sert aux chemins (points de montage) et aux noms de sondes.

- Props : `text`, `tail` (12 par défaut)
- Notes : se réduit dans son parent flexible (`min-width: 0`). FIX:01M4EPX88BTXFX1PX7E57WGK38. Tests : `e2e/hrt47.spec.ts`.
- Accessibilité (revue de #63) : la troncature est visuelle seulement ; le texte COMPLET est dans le document (`sr-only`), les deux moitiés affichées sont `aria-hidden` : un lecteur d'écran lit le chemin entier, aucun rôle inventé. La mesure suit le texte quand il change (observateur rattaché à la moitié « début » à chaque changement).
