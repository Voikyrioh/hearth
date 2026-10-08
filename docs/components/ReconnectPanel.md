# ReconnectPanel

Organisme · `apps/desktop/src/components/organisms/ReconnectPanel.vue`

Serveur sans session (logique dans `composables/useReconnect.ts` ; recréé par serveur grâce à `:key` dans `ServerLayout`, la saisie de l'un ne part jamais vers l'autre) (expirée, déconnexion, première connexion interrompue, mot de passe mémorisé refusé) : formulaire de connexion au-dessus de la dernière vue (périmée), identifiant prérempli. Raison `expired` : « Ta session a expiré. » puis « Rentre ton mot de passe pour reprendre. », bouton « Me reconnecter » (BR-RESIL-012, 013) ; mot de passe mémorisé refusé : « Rentre ton mot de passe pour reprendre. », aucun message bloquant (BR-CONN-017) ; accès révoqué : « Ton compte n'est plus accessible. », « Connecte-toi avec un compte valide. » et le bouton « Utiliser un autre compte », qui ouvre le formulaire, identifiant vide (BR-RESIL-014). Jamais modal (BR-RESIL-011).

- Props : `server`, `reason`, `revoked`
- Événements et slots : aucun
- Notes : Monté par `ServerLayout`. Tests : `connect.test.ts`, `router/connect.test.ts`, `e2e/offline.spec.ts`.

HRT-35 : panneau EN SURIMPRESSION, centré horizontalement et verticalement dans la zone de contenu, premier dans le DOM (atteint en premier au clavier), au-dessus par `z-index`, fond opaque et ombre ; il ne repousse plus la dernière vue. « Utiliser un autre compte » reste réservé à l'accès révoqué (BR-RESIL-014).
