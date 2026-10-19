# ServiceLifecycleDialog

Organisme · `apps/desktop/src/components/organisms/ServiceLifecycleDialog.vue`

La fenêtre de démarrage, d'arrêt et de redémarrage d'un service (HRT-65) : une seule fenêtre pour l'effet et la confirmation du socle (`AdminActDialog`, genres `service_start`, `service_stop`, `service_restart`, couverts par l'élévation de cinq minutes). Arrêter et redémarrer disent l'effet avec le délai d'arrêt réel du service, puis les avertissements du service (créé hors de Hearth, « Redémarre toujours », géré par le système). Démarrer n'a que le champ du socle. Si le service change sous la fenêtre (BR-LIFE-022) elle le dit et la confirmation devient inactive.

- Props : `open`, `serverId`, `gesture`, `service` (à l'ouverture), `current` (dans la liste vivante, `null` : disparu), `locked`, `perform`
- Événements : `close`
- Notes : tenue par la page Services (`composables/useServiceLifecycle.ts`). Tests : `pages/ServicesLifecycle.test.ts`.
