# AttackModePanel

Organisme · `apps/desktop/src/components/organisms/AttackModePanel.vue`

Carte « Mode attaque » de la page Sécurité (HRT-26, BR-TRUST-010, 018, 029) : titre, étiquette d'état écrite (« Inactif », « Actif », « Suspendu »), explication, et UN bouton qui active ou désactive (secondaire quand le mode est actif). Quand le mode est actif ou suspendu, la carte dit aussi que la sortie automatique est repoussée par les postes légitimes bloqués autant que par l'attaquant, qu'un poste connu seulement par son adresse est bloqué dès qu'un essai raté a eu lieu depuis cette adresse, et que la preuve d'un poste dont l'adresse change entre le défi et la connexion est ignorée. Bouton indisponible : focusable (`aria-disabled`), infobulle ET raison écrite sous le bouton (une seule raison, dans l'ordre : Lecture seule, agent trop ancien, poste sans clé enregistrée avec le texte de la spec) ; « Se reconnecter » si le poste n'a pas de clé et aucun mot de passe mémorisé.

- Props : `mode`, `block`, `minutes`, `busy`, `canReconnect`
- Événements et slots : `change`, `reconnect`
- Notes : la confirmation (avec mot de passe) est portée par `AttackModeDialog`, ouverte par `security.ask` dans le gabarit. Tests : `pages/SecurityMode.test.ts`, `e2e/attack-mode.spec.ts`.

HRT-39 (C34, FIX-01M4DJZAFYE77R5MKKA2NV6CE3) : éteint, le texte dit ce que le mode FERA ; actif, une phrase (depuis quand) et les règles repliées dans « Comment ça marche ».
- HRT-38 (C46) : hors « Connecté », l'état illisible dit « Pas encore chargé… » sans « Réessayer » (celui du bandeau suffit).
