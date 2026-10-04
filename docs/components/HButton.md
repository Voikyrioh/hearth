# HButton

Atome · `apps/desktop/src/components/atoms/HButton.vue`

Bouton d'action. Désactivé = `aria-disabled` et non `disabled` : il garde le focus clavier et son infobulle `hint`, qui explique pourquoi il est inactif. Un seul bouton `primary` par vue.

- Props : `variant` (`primary` par défaut, `secondary`, `ghost` : bouton d'icône), `disabled`, `hint` (infobulle), `type`
- Événements et slots : événement `click` (jamais émis si désactivé) ; slot par défaut
- Notes : Jetons : `--ac`, `--on-ac`, `--bd`, `--card-2`. Focus : contour global de `base.css`. Tests : `HButton.test.ts`.
