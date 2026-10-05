# ReconnectPanel

Organisme · `apps/desktop/src/components/organisms/ReconnectPanel.vue`

Serveur sans session (logique dans `composables/useReconnect.ts` ; recréé par serveur grâce à `:key` dans `ServerLayout`, la saisie de l'un ne part jamais vers l'autre) (expirée, déconnexion, première connexion interrompue, mot de passe mémorisé refusé) : formulaire de connexion au-dessus de la dernière vue (périmée), identifiant prérempli. Raison `expired` : « Session expirée. Reconnecte-toi. » ; mot de passe mémorisé refusé : aucun message bloquant (BR-CONN-017) ; accès révoqué : « Accès révoqué. Contact l'administrateur. », sans formulaire.

- Props : `server`, `reason`, `revoked`
- Événements et slots : aucun
- Notes : Monté par `ServerLayout`. Tests : `connect.test.ts`, `router/connect.test.ts`.
