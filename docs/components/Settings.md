# Settings

Page · `apps/desktop/src/pages/Settings.vue`

Réglages : section « Général » puis section « Mises à jour » (`UpdatePanel`, HRT-16) (pas de barre d'onglets) : interrupteur « Lancer Hearth au démarrage de Windows » (BR-CLIENT-007), bouton « Ouvrir le dossier des journaux » et version de l'application. Lit et écrit par le store `settings` (commandes typées) ; réglages illisibles, version indisponible et dossier des journaux inaccessible s'affichent séparément. Route `/settings`.

- Props : aucune
- Événements et slots : aucun
- Notes : D'autres sections et les cartes d'état par serveur arrivent avec leurs tickets ; la barre de sections reviendra avec la seconde. Tests : `Settings.test.ts`.

- HRT-12 : section « Notifications » : « Notifier quand un serveur devient hors ligne ou revient » (activé par défaut, BR-RESIL-015) ; commandes `get_notify_on_link_change` / `set_notify_on_link_change`.
