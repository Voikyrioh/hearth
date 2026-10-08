# MachineCard

Organisme · `apps/desktop/src/components/organisms/MachineCard.vue`

Sections « Machine » (Nom, Système, Processeur, Mémoire, Disques) et « Durée de fonctionnement » (3 j 4 h 12 min). La liste des disques suit l'échantillon courant. BR-DASH-001, 012, 014.

- Props : `entry` (`ServerMachine` du store `dashboard`)
- Événements et slots : aucun
- Notes : Tests : `pages/Dashboard.test.ts`.
- HRT-47 (S1b) : chaque disque (point de montage tronqué au milieu, taille) tient dans la carte. FIX:01M4EPX88BTXFX1PX7E57WGK38.
- HRT-47 (S5, PROPOSITION à montrer à Voiky) : grille à deux colonnes (libellés à gauche, valeurs à droite), un rang par disque (point de montage tronqué au milieu, taille alignée à droite), le processeur sur deux lignes (modèle ; cœurs et fréquence). Jetons du design system seulement. Test : `e2e/hrt47-machine.spec.ts` (captures avant/après `smoke-machine-*`).
- Revue de #64 : les disques sont une vraie liste (`ul` dans le `dd`, un `li` par disque : point de montage puis taille) ; une valeur longue d'un seul tenant (nom de machine, modèle de processeur) se coupe dans sa colonne (`overflow-wrap`). Garde de vide autour des jauges (dashboard.spec) vérifiée verte à 1920 et 2560.
