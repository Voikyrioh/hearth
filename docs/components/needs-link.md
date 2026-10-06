# needs-link (prop `needsLink` et `useNeedsLink`)

Composable + prop · `apps/desktop/src/composables/useNeedsLink.ts` ; props de `HButton` et `HToggle`

L'unique façon de désactiver une action qui exige le serveur (BR-RESIL-008). `useNeedsLink(() => needsLink)` rend l'explication à montrer quand le lien du serveur courant (`servers.currentId`) n'est pas « Connecté », ou que le rôle ne suffit pas, sinon `null`. Le composant l'ajoute à son propre état : **désactivé final = `disabled || busy || raison`**, calculé dans le composant (`aria-disabled`, clic ignoré, focus clavier conservé), et la raison s'affiche par `HTooltip` (survol et focus, `aria-describedby`). Personne d'autre n'écrit `aria-disabled` : plus de directive sur le DOM (le lien qui tombe pendant une requête occupée ne peut plus laisser un aspect actif).

- Valeur de la prop : `true`, ou `{ role: 'admin' }` pour exiger aussi le rôle administrateur ; `{ server: id }` désigne le serveur concerné quand la page n'est pas celle d'un serveur (les réglages listent tous les serveurs : `OwnAccountCard`), sinon c'est le serveur courant
- Textes (`fr.ts`, groupe `needs`) : « Indisponible tant que le serveur est hors ligne. », « Indisponible pendant la reconnexion au serveur. », « Indisponible : ta session a expiré. », « Indisponible : ton compte n'est plus accessible. », « Réservé aux administrateurs. », « Indisponible : aucun serveur sélectionné. »
- Notes : le rôle est testé avant l'état du lien. Un nouveau contrôle qui exige le serveur reçoit la prop `needsLink` (ou appelle `useNeedsLink` pour le sien ; `HInput` n'en a pas tant qu'aucun champ n'en a besoin) : jamais de désactivation maison. Tests : `HButton.test.ts`, `shell.test.ts`, `e2e/shell.spec.ts`, `e2e/offline.spec.ts`. Un bouton qui lance une action passe par `useServerAction.run(perform)` (`composables/useServerAction.ts`, `perform` appelle UNE commande typée) : résultat inconnu à la coupure, notifications discrètes, jamais de rejeu (BR-RESIL-009).

```vue
<HButton needs-link @click="remove">Supprimer</HButton>
<HButton :needs-link="{ role: 'admin' }" @click="update">Mettre à jour l'agent</HButton>
```
