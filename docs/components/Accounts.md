# Accounts

Page · `apps/desktop/src/pages/Accounts.vue`

Comptes (HRT-13), réservée aux administrateurs (pas d'entrée de menu pour Lecture seule, route fermée par `meta.adminOnly`, l'agent refuse de toute façon). Bouton « Ajouter un compte » dans l'en-tête (`needs-link` avec le rôle administrateur). États : chargement, liste vide (« Aucun compte n'existe pour le moment. Crée-en un pour commencer. » avec le bouton), liste (`AccountTable`), refus de l'agent (« Tu n'as pas la permission pour accéder à la gestion des comptes »), lecture impossible (« Réessayer »). Toute action passe par `useAccountActions` (donc `useServerAction` : résultat inconnu à la coupure, jamais rejouée) ; la liste se relit à chaque retour du lien et à chaque issue d'opération incertaine (store `accounts`). Suppression : `ConfirmDialog` qui nomme le compte, avec l'erreur dans la fenêtre ; « dernier administrateur » : notification, fenêtre fermée.

- Props : aucune
- Événements et slots : aucun
- Notes : Route `/servers/:id/accounts`. Tests : `pages/Accounts.test.ts`, `shell.test.ts`, `e2e/accounts.spec.ts`.

HRT-30 : changer un rôle, fermer des sessions et supprimer un compte ouvrent chacun `AdminActDialog` (kinds `account_role`, `sessions_revoke`, `account_delete`) ; plus aucune action de ligne ne part sans fenêtre.
