# ModsTab

Organisme · `apps/desktop/src/components/organisms/ModsTab.vue`

L'onglet « Mods » d'un serveur Minecraft installé par Hearth (HRT-261, HRT-262, BR-MOD-020 à 033, maquettes Main et SansMods) : titre et phrase de bilan, version de Minecraft et sa source, phrases « dernière version à chaque démarrage » et « pack Modrinth », bandeau pour un mod incompatible ACTIVÉ (ou installé deux fois), « Lecture des mods en cours » pour un gros serveur (relecture automatique), la table (`ModRow`), les « Mods retirés », la légende des origines et la phrase de BR-MOD-009. Les moteurs sans mods (Vanilla, plugins) disent leur phrase et proposent « Ajouter un serveur de jeu ». Tenu par `composables/useServiceMods.ts` (lecture, lot de gestes) ; règles pures `services/mods.ts` ; posé par `ServiceDetail` quand la fiche du service offre la voie `drop_jars` (`composables/useServiceDoors.ts`).

- Props : `serverId`, `service`, `gate`, `rightsReasonId`, `connected`, `consoleServed`, `consoleOpen`
- Événements : `addServer`, `openConsole`, `openLogs`
- Écrans : la liste, le dépôt (`ModsDepositDialog`), le contrôle (`ModsCheckList`), « Vérifiez avant d'appliquer » (`ModsApplyDialog`), le suivi et l'issue (`ModsProgress`), la liste des joueurs (`ModsPlayersList`).
- Notes : le lot vit au niveau du service (`useServiceMods` : `hasKeptBatch`, `dropKeptBatch`) et survit au changement d'onglet ; le détail demande confirmation avant de QUITTER le détail (« Retour », Échap, bouton précédent, autre service). Rien n'est écrit avant « Appliquer ». Table à colonnes fixes : moteur et version visée cachés sous 860 px de zone, noms de fichiers tronqués avec `title`. Chaque écran s'ouvre en haut, sous l'en-tête collant (nom et onglets). Mods qui empêcheront le démarrage (incompatible, doublon, dépendance absente) : bandeau, rappelés dans « Vérifiez avant d'appliquer » ; tests `components/organisms/modsTab.test.ts` et `pages/ServicesMods.test.ts`.
