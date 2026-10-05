# HButton

Atome · `apps/desktop/src/components/atoms/HButton.vue`

Bouton d'action. Désactivé = `aria-disabled` et non `disabled` : il garde le focus clavier et son infobulle `hint`, qui explique pourquoi il est inactif. Occupé (`busy`) : indicateur d'attente, `aria-busy`, clic ignoré. Un seul bouton `primary` par vue. Pour désactiver selon le lien ou le rôle, ne pas poser `disabled` à la main : utiliser `v-needs-link`.

- Props : `variant` (`primary` par défaut, `secondary`, `danger`, `ghost` : bouton d'icône), `size` (`sm`, `md`, `lg`), `disabled`, `busy`, `solid` (destructeur plein, réservé à la confirmation finale), `hint` (infobulle), `type`
- Événements et slots : événement `click` (jamais émis si désactivé ou occupé) ; slot par défaut
- Notes : Jetons : `--ac`, `--on-ac`, `--crit`, `--bd`, `--card-2`, `--control-sm`, `--control-lg`. Focus : contour global de `base.css`. Tests : `HButton.test.ts`, `atoms.test.ts`.
