# HButton

Atome · `apps/desktop/src/components/atoms/HButton.vue`

Bouton d'action. Désactivé = `aria-disabled` et non `disabled` : il garde le focus clavier et son infobulle `hint`, qui explique pourquoi il est inactif. Occupé (`busy`) : indicateur d'attente, `aria-busy`, clic ignoré. Un seul bouton `primary` par vue. Pour désactiver selon le lien ou le rôle, ne pas poser `disabled` à la main : utiliser la prop `needsLink` (voir `needs-link.md`). L'état final est `disabled || busy || raison du lien`, calculé dans le composant.

- Props : `variant` (`primary` par défaut, `secondary`, `danger`, `ghost` : bouton d'icône), `size` (`sm`, `md`, `lg`), `disabled`, `busy`, `solid` (destructeur plein, réservé à la confirmation finale), `needsLink` (`true` ou `{ role: 'admin' }`), `hint` (explication d'un blocage fixe, en infobulle `HTooltip`, pas en `title`), `tipPlacement` (`start`, `center`, `end`), `type`
- Événements et slots : événement `click` (jamais émis si désactivé ou occupé) ; slot par défaut
- Notes : Jetons : `--ac`, `--on-ac`, `--crit`, `--bd`, `--card-2`, `--control-sm`, `--control-lg`. Les attributs passés (`aria-label`, `data-*`) vont au `<button>` (`inheritAttrs: false`), l'enveloppe est un `<span>` de `HTooltip`. Focus : contour global de `base.css`. Tests : `HButton.test.ts`, `atoms.test.ts`.
- HRT-40 (C42) : prop `reasonBelow` : la raison du blocage (`hint` d'un bouton `disabled`, ou celle du lien) est aussi écrite en texte permanent sous le bouton (`data-button-reason`). L'enveloppe `display: contents` ne change aucune mise en page sans cette prop.
