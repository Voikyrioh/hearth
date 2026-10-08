# TrustedDeviceTable

Organisme · `apps/desktop/src/components/organisms/TrustedDeviceTable.vue`

« Tes postes de confiance » (HRT-23, ADR-0023) : la carte qui liste les postes que le serveur reconnaît pour le compte de la session. Titre, compteur « {n} sur {max} » (pictogramme `info` quand la liste est pleine), tableau (`<caption>`, `<th scope>`) : nom du poste (étiquette « Ce poste » sur celui-ci, qui passe en premier ; ensuite la dernière utilisation la plus récente), dernière utilisation (« il y a 2 jours », « Jamais » si absente), « Retirer ». « Retirer » est un `HButton` destructeur `needs-link` : grisé avec son explication sur ce poste (« Tu ne peux pas retirer le poste que tu utilises. ») et, pour tous les postes, quand ce PC n'a pas de clé inscrite (aucun poste « courant » : la preuve de la clé manque ; ligne d'avertissement « Ce poste n'est pas encore enregistré… » en tête de carte).

États : chargement (squelette fixe, `aria-busy`, « Chargement de tes postes… »), liste, vide (`EmptyState`), 8 sur 8 (ligne « Tu as atteint 8 postes sur 8… » ; si aucun poste n'est en main, l'aide des voies de secours : administrateur qui change le mot de passe, `hearth-agent account passwd`), erreur de lecture (`role="alert"` et « Réessayer ») sans liste connue, agent trop ancien (« Cette fonction n'existe pas encore sur ce serveur. Mets l'agent à jour. »). Données périmées : le gabarit de serveur désature la page (`StaleSurface`), les boutons sont désactivés par `needs-link`. Sous 1 200 px : défilement horizontal interne ; sous 1 000 px : chaque poste devient un bloc empilé.

- Props : `status` (`loading | ready | unsupported | error`), `devices` (`TrustedDevice[]`), `max`, `busy`
- Événements et slots : `remove(device)`, `retry`
- Notes : aucune clé, empreinte de clé, défi ni signature n'existe côté interface (noms, dates, adresse, booléens). Tests : `pages/Security.test.ts`, `e2e/security.spec.ts`.
- HRT-38 (C46) : hors « Connecté », une liste jamais lue dit « Pas encore chargé, sera disponible quand le serveur reviendra. » sans « Réessayer ».
