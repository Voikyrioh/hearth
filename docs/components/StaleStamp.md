# StaleStamp

Molécule · `apps/desktop/src/components/molecules/StaleStamp.vue`

Âge du dernier contact, mis à jour en direct : « Vu il y a 12 s », « Vu il y a 2 min », « Vu il y a 3 h », « Vu il y a 2 j », « Jamais vu ». BR-RESIL-007.

- Props : `lastContactAt` (ms, ou `null`)
- Événements et slots : aucun
- Notes : Horloge partagée `composables/useNow.ts` (un seul minuteur pour toute l'interface). DM Mono, `--tx3`. Tests : `molecules.test.ts`, `format.test.ts`.
