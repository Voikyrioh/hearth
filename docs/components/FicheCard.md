# FicheCard

Molécule · `apps/desktop/src/components/molecules/FicheCard.vue`

Une fiche dans la liste du Catalogue (HRT-102, BR-FICHE-003) : le nom du jeu, une phrase, « Livrée avec Hearth {version} » et « Voir la fiche ». Pas d'illustration, pas de bouton « Installer » (un seul chemin, depuis la fiche), aucune mention de compte. Un jeu sans serveur pour ce système est atténué et le dit. Les textes viennent de la fiche et sont montrés tels quels (`safeText`).

- Props : `card` (`FicheCard`), `agentVersion` (`string | null` : sans version, « Livrée avec Hearth »)
- Événements et slots : `open`
- Notes : Posé par `pages/Catalogue.vue`. Tests : `pages/Catalogue.test.ts`.
