# EmptyState

Molécule · `apps/desktop/src/components/molecules/EmptyState.vue`

Placeholder d'une vue vide : illustration, titre (Sora 30), texte, action. Largeur max 420 px, centré. L'illustration vient de la collection `src/assets/illustrations/` (images générées choisies par le détenteur, PNG à fond transparent produits par `npm run build:illustrations`, ADR-0027) : décorative (`alt` vide), 220 px (`md`) ou 280 px (`lg`) par jetons.

- Props : `title`, `text`, `heading` (`h1` par défaut, `h2` quand la vue a déjà son `h1`), `illustration` (`firstLaunch`, `journal`, `offline` ; absente : rien), `size` (`md` par défaut, `lg`)
- Événements et slots : slots `illustration` (prime sur la prop), `action` (tous deux optionnels)
- Notes : Avec illustration : l'accueil (`firstLaunch`, `lg`), le journal vide sans filtre (`journal`), le tableau de bord d'un serveur hors ligne sans mesure (`offline`) ; « Comptes » vide n'en a pas (illustration à venir, ticket à part ; une ligne de la table, `assets/illustrations/screens.ts`, ADR-0027). Les images se posent sur le fond de page `--bg` sans cadre : tout nouvel emplacement sur une autre surface (carte) reste correct grâce à la transparence (HRT-31). Tests : `EmptyState.test.ts`.
