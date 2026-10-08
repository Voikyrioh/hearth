# StepTrail

Molécule · `apps/desktop/src/components/molecules/StepTrail.vue`

Fil des étapes de l'assistant d'ajout : « Adresse », « Empreinte », « Connexion ». Étape courante en pastille braise, étapes faites cochées en `--ok` ; l'état est dit en texte pour les lecteurs d'écran (`aria-current="step"`).

- Props : `steps`, `current` (index), `label`
- Événements et slots : aucun
- Notes : Tests : `connect.test.ts`, `e2e/connect.spec.ts`.
- HRT-47 (S3) : chiffre centré par sa hauteur de capitale (`text-box`), interligne 1. FIX:01M4EPX90N9QNNPCHVNB6MCDDF.
