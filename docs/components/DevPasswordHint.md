# DevPasswordHint

Organisme · `apps/desktop/src/components/organisms/DevPasswordHint.vue`

Rappel du mot de passe du compte simulé, sous le champ « Votre mot de passe » de la confirmation d'un acte (revue UX du 2026-10-24, constat 1) : « Simulateur : le mot de passe de ce compte est … ». Sans lui, la revue ne pouvait ni lancer un déploiement ni voir la suite. N'existe qu'avec le pont SIMULÉ.

- Props : `serverId`
- Notes : chargé par `AdminActDialog` seulement derrière `import.meta.env.DEV` (composant asynchrone) : absent du binaire livré. `npm run check:dist` refuse dans `dist/` le mot de passe d'exemple et ce libellé. Son texte reste dans le composant (hors `fr.ts`) pour ne pas entrer dans le bundle livré. Test : `components/organisms/adminAct.test.ts`, `e2e/ux-2026-10-24.spec.ts`.
