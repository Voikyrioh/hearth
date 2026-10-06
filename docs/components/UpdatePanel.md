# UpdatePanel

Organisme · `apps/desktop/src/components/organisms/UpdatePanel.vue`

Section « Mises à jour » de la page Réglages : état (« Tu es à jour » ou « Nouvelle version disponible : 1.1.0 »), « Dernière vérification : il y a 2 heures » (date de la dernière vérification réussie ; jamais de message d'erreur sans Internet, BR-UPDATE-007), « Vérifier maintenant » (désactivé et « Vérification en cours… » pendant la vérification, BR-UPDATE-026) et, si une version est connue, « Notes de version » et « Mettre à jour maintenant ». Après un échec de vérification on ne prétend pas être à jour.

- Props : aucune (lit le store `updates`)
- Événements et slots : aucun
- Notes : `data-updates-panel`, `data-updates-status`, `data-updates-last` pour les tests. Tests : `components/organisms/updates.test.ts`, `e2e/updates.spec.ts`.
