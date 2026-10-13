# HTooltip

Atome · `apps/desktop/src/components/atoms/HTooltip.vue`

Bulle d'explication au survol et au focus clavier (`role="tooltip"`), fermée par Échap. Le slot reçoit `describedby` à poser sur le contrôle. L'enveloppe existe même sans texte (le contrôle n'est jamais recréé quand l'explication apparaît : le focus clavier reste).

- Props : `text` (sans texte, pas de bulle), `placement` (`start`, `center`, `end`), `side` (`bottom`, `top`)
- Événements et slots : slot par défaut (`{ describedby }`)
- Notes : C'est l'unique affichage de la raison d'un blocage (lien, rôle, `hint`) : `HButton`, `HToggle` et `HInput` l'utilisent. Tests : `atoms.test.ts`.
- HRT-134 : la bulle est téléportée dans `body` (Teleport) et placée à la main en position fixe sous (ou sur) le contrôle, ramenée dans la fenêtre (marge de 8 px) et suivie au défilement et au redimensionnement ; jamais coupée par un conteneur à `overflow` ou à `container-type`, ne cause aucune barre de défilement. Les tests la lisent dans `document.body` (`src/test/tooltip.ts` : `tooltip()`, `hasTooltip()`, `tooltipFor(contrôle)`), `src/test/setup.ts` démonte les composants après chaque test. Test : `pages/uxFixes.test.ts`, `e2e/hrt134-141-ux.spec.ts`.
- Revue PR 91 : la bulle est un `popover="manual"` ouvert par `showPopover()` (fermé par `hidePopover()`) : elle entre dans la couche supérieure du navigateur, donc passe AU-DESSUS d'une fenêtre modale ouverte par `showModal()` (un `z-index` n'y peut rien). Cible : WebView2 Evergreen (Chromium 114 et plus, installé par l'installateur). Sans `popover` (jsdom, vieux moteur), `v-show` suffit à cacher la bulle. Elle suit son contrôle même s'il bouge sans défilement (lecture de sa position à chaque image tant qu'elle est ouverte), et se ferme si le contrôle disparaît ou sort du cadre de son conteneur (`IntersectionObserver`). Contenu : texte seul, jamais de contrôle interactif (la bulle ignore la souris). Tests : `e2e/hrt134-141-ux.spec.ts` (bulle dans une fenêtre modale réelle, suivi).
