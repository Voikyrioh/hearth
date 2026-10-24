# FichePortField

Molécule · `apps/desktop/src/components/molecules/FichePortField.vue`

Un port d'une fiche à l'étape « Réponses » (HRT-102, BR-FICHE-019) : le port du serveur que les joueurs utiliseront, proposé par la fiche. Le conflit (qui tient quoi) est dit par l'agent (`verdict`) et jamais recalculé : une erreur bloque, un avertissement non, un port libre proche est proposé (« Utiliser le port … »). Un port facultatif de la fiche se publie par une case ; tant qu'elle n'est pas cochée, aucun champ n'est montré. « Ce port sera accessible à tout appareil qui peut joindre votre serveur. » est toujours dit.

- Props : `port` (`FichePort`), `modelValue`, `error`, `verdict` (`PortVerdict`), `changed`, `waiting`, `optional`, `on`
- Événements et slots : `update:modelValue`, `update:on`, `suggest`, `blur`
- Notes : Tests : `pages/InstallFromFiche.test.ts`.
