# DisksCard

Organisme · `apps/desktop/src/components/organisms/DisksCard.vue`

Section « Disques » : par disque, occupation (jauge linéaire, pourcentage, marque d'alerte), « Utilisé / Total », « Libre : X Go » ; courbe du disque le plus plein. Un disque monté ou retiré apparaît ou disparaît seul (BR-DASH-012).

- Props : `entry`
- Événements et slots : aucun
- Notes : Tests : `pages/Dashboard.test.ts`.
- HRT-47 (S1b) : nom et point de montage en `HMiddleText` (jamais en colonne ni hors de la carte), pourcentage sur une ligne. FIX:01M4EPX88BTXFX1PX7E57WGK38.
