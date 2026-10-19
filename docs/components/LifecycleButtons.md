# LifecycleButtons

Molécule · `apps/desktop/src/components/molecules/LifecycleButtons.vue`

Les boutons de pilotage d'un service (HRT-65) : Démarrer, Arrêter, Redémarrer. Un geste que l'état n'offre pas est absent (BR-LIFE-003). Un geste bloqué par le droit, le moteur, le verrou d'une action ou le lien reste visible, `aria-disabled`, focusable ; la raison est écrite sous le groupe (`data-lifecycle-reason`, jamais une infobulle seule), avec « Lancée par {compte}. » pour le verrou. Sans geste offert, la phrase qui le dit tient leur place (`data-lifecycle-note`).

- Props : `controls` (calculé par `services/lifecycle.ts::controlsOf`), `name`
- Événements : `gesture` (le geste demandé)
- Notes : aucune règle ici. Tests : `pages/ServicesLifecycle.test.ts`.
