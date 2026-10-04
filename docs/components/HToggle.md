# HToggle

Atome · `apps/desktop/src/components/atoms/HToggle.vue`

Interrupteur (`role="switch"`, `aria-checked`). Même convention que `HButton` : désactivé ou occupé = `aria-disabled` (focus conservé), jamais `disabled` natif. Son nom accessible vient d'un `aria-labelledby` posé par l'appelant.

- Props : `modelValue`, `disabled`, `busy` (commande en cours : second clic ignoré, `aria-busy`), `hint` (raison, en infobulle)
- Événements et slots : `update:modelValue`
- Notes : Tests : `HToggle.test.ts`.
