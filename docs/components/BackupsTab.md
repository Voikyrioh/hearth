# BackupsTab

Organisme · `apps/desktop/src/components/organisms/BackupsTab.vue`

L'onglet « Sauvegardes » d'un service (HRT-98, BR-BACKUP-001 à 015, 026 à 028) : cartes « Espace » et « Éléments ignorés », bouton « Sauvegarder », la sauvegarde qui court (progression, annulation), l'issue dite jusqu'à sa fermeture, la table, états vide, chargement et échec. Tenu par `composables/useServiceBackups.ts` ; posé par `ServiceDetail` quand la porte `backupsDoor` est ouverte.

- Props : `serverId`, `service`, `gate`, `lock`, `rightsReasonId`, `connected`, `engineReady`
- Événements : aucun
- Notes : tests `pages/ServicesBackups.test.ts`.
