# ToastStack

Molécule · `apps/desktop/src/components/molecules/ToastStack.vue`

Notifications discrètes empilées en bas à droite : 3 visibles au plus, compteur sur les répétitions (« 5 fois »), jamais bloquantes (aucun focus volé), fermables, annoncées (`aria-live="polite"`). BR-RESIL-011, 018.

- Props : aucune (lit le store `toasts`)
- Événements et slots : aucun
- Notes : Pousser une notification : `useToastsStore().push({ kind, message })`. Tests : `molecules.test.ts`, `link-stores.test.ts`.
