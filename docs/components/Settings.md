# Settings

Page · `apps/desktop/src/pages/Settings.vue`

Réglages, dans l'ordre du design (`design-ecrans-socle.md`) : à gauche « Général », « Client » (notifications du lien et version), « Mises à jour » (`UpdatePanel`, HRT-16) ; à droite « État du serveur : {nom} » (`AgentUpdateCard`, HRT-17 : versions, mise à jour de l'agent), une carte par serveur, puis « Mon compte », une carte par serveur (`OwnAccountCard`, HRT-13, tous les rôles). Pas de barre d'onglets. « Général » : interrupteur « Lancer Hearth au démarrage de Windows » (BR-CLIENT-007), bouton « Ouvrir le dossier des journaux » et version de l'application (rangée dans « Client »). Lit et écrit par le store `settings` (commandes typées) ; réglages illisibles, version indisponible et dossier des journaux inaccessible s'affichent séparément. Route `/settings`.

- Props : aucune
- Événements et slots : aucun
- Notes : D'autres sections arrivent avec leurs tickets ; la barre de sections reviendra avec la suivante. Tests : `Settings.test.ts`.

- HRT-12 : réglage de notifications (rangé dans « Client » depuis HRT-13, préférence du client) : « Notifier quand un serveur devient hors ligne ou revient » (activé par défaut, BR-RESIL-015) ; commandes `get_notify_on_link_change` / `set_notify_on_link_change`.
- HRT-26 : la section « Client » porte la ligne « Alertes de sécurité » (réglage séparé de « Notifier quand un serveur devient hors ligne ou revient », activé par défaut, BR-TRUST-033). Test : `pages/SecuritySettings.test.ts`.
