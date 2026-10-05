# v-needs-link

Directive · `apps/desktop/src/directives/needsLink.ts`

L'unique façon de désactiver une action qui exige le serveur (BR-RESIL-008). Tant que le lien du serveur courant (`servers.currentId`) n'est pas « Connecté », ou que le rôle ne suffit pas, l'élément devient inerte : `aria-disabled="true"` (il garde le focus clavier, convention de `HButton`), clic bloqué en phase de capture avant tout gestionnaire du composant, infobulle (`title`) qui dit pourquoi. Le retour du lien le réactive seul et restitue l'`aria-disabled` et le `title` d'origine.

- Valeur : aucune, ou `{ role: 'admin' }` pour exiger aussi le rôle administrateur
- Textes (`fr.ts`, groupe `needs`) : « Indisponible tant que le serveur est hors ligne. », « Indisponible pendant la reconnexion au serveur. », « Indisponible : ta session a expiré. », « Indisponible : ton compte n'est plus accessible. », « Réservé aux administrateurs. », « Indisponible : aucun serveur sélectionné. »
- Notes : Enregistrée dans `main.ts` (`app.directive("needs-link", vNeedsLink)`). Le rôle est testé avant l'état du lien. Pose `data-needs-link` (clé du texte) pour les tests. Tests : `needsLink.test.ts`, `shell.test.ts`, `e2e/shell.spec.ts`.

```vue
<HButton v-needs-link @click="remove">Supprimer</HButton>
<HButton v-needs-link="{ role: 'admin' }" @click="update">Mettre à jour l'agent</HButton>
```
