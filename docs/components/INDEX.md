# Composants — Hearth

Interface Vue 3, atomic design. Aucune logique réseau dans la vue : tout passe par le pont de liaison (`src/link/`, interface `LinkBridge`) et les stores Pinia `servers`, `link`, `toasts`, `dashboard`. Une fiche par composant.

## Atomes

- [`HButton`](./HButton.md) — Bouton (principal, secondaire, destructeur, icône ; tailles ; occupé).
- [`HInput`](./HInput.md) — Champ texte (libellé, aide, erreur, focus braise).
- [`HPasswordInput`](./HPasswordInput.md) — Champ mot de passe (afficher/masquer).
- [`HCheckbox`](./HCheckbox.md) — Case à cocher.
- [`HTag`](./HTag.md) — Étiquette courte.
- [`HIcon`](./HIcon.md) — Jeu d'icônes SVG.
- [`HLogo`](./HLogo.md) — Logo Hearth.
- [`HTooltip`](./HTooltip.md) — Bulle d'explication (survol, focus).
- [`HMiddleText`](./HMiddleText.md) — Texte long tronqué au milieu, complet au survol et au clavier.
- [`HSpinner`](./HSpinner.md) — Indicateur d'attente.
- [`HToggle`](./HToggle.md) — Interrupteur booléen.
- [`HSelect`](./HSelect.md) — Liste déroulante native (un choix).
- [`HSegmented`](./HSegmented.md) — Choix exclusif (radiogroup).
- [`HGaugeArc`](./HGaugeArc.md) — Arc de jauge de 270° (SVG).
- [`HAreaChart`](./HAreaChart.md) — Courbe pleine à une ou deux séries (SVG), trous pour les mesures absentes.
- [`HBars`](./HBars.md) — Barres verticales (SVG), une par cœur.
- [`HMeter`](./HMeter.md) — Jauge linéaire (SVG), occupation d'un disque.
- [`HSelect`](./HSelect.md) — Liste déroulante native (libellé, masquable).
- [`HTextarea`](./HTextarea.md) — Zone de texte qui grandit avec la saisie (HRT-69).

## Molécules

- [`LinkStatePill`](./LinkStatePill.md) — Pastille d'état du lien (5 états).
- [`ErasurePendingNote`](./ErasurePendingNote.md) — Note « Effacement en attente » de la page Sécurité (administrateur, rien à faire).
- [`SecurityAlertCard`](./SecurityAlertCard.md) — Carte « Ce qui se passe » de la page Sécurité (alerte : depuis quand, combien de comptes, quoi faire).
- [`SecurityBanner`](./SecurityBanner.md) — Base des bandeaux de sécurité (alerte, mode attaque, suspendu).
- [`ServerAvatar`](./ServerAvatar.md) — Avatar de serveur (initiales, anneau, pastille d'état).
- [`MultiSelect`](./MultiSelect.md) — Liste déroulante à choix multiple (cases à cocher).
- [`AuditDetailDialog`](./AuditDetailDialog.md) — Une entrée du journal en entier.
- [`StaleStamp`](./StaleStamp.md) — « Vu il y a 12 s », en direct.
- [`StaleSurface`](./StaleSurface.md) — Enveloppe des données périmées (désaturées et datées).
- [`ToastStack`](./ToastStack.md) — Notifications empilées (3 visibles, compteur).
- [`ConfirmDialog`](./ConfirmDialog.md) — Confirmation modale (piège à focus, défaut sûr).
- [`ProofTag`](./ProofTag.md) — D'où vient un chiffre de ressource d'une fiche : mesuré, au repos, éditeur, non vérifié (HRT-102).
- [`FicheCard`](./FicheCard.md) — Une fiche dans la liste du Catalogue (HRT-102).
- [`QuestionField`](./QuestionField.md) — Une question d'installation dans le champ de son genre, lue dans la fiche (HRT-102).
- [`VariantChoice`](./VariantChoice.md) — Le choix de la variante d'une fiche (HRT-102).
- [`FichePortField`](./FichePortField.md) — Un port d'une fiche : le port du serveur, le constat de l'agent, le port libre proposé (HRT-102).
- [`FormDialog`](./FormDialog.md) — Fenêtre de formulaire (champs figés pendant l'envoi, erreur dans la fenêtre).
- [`PasswordRules`](./PasswordRules.md) — Critères du mot de passe en direct (coche ou croix et texte).
- [`ReleaseNotesDialog`](./ReleaseNotesDialog.md) — Notes de version en texte brut (boîte modale).
- [`ErrorBoundary`](./ErrorBoundary.md) — Frontière d'erreur d'une page.
- [`EmptyState`](./EmptyState.md) — Placeholder de vue vide.
- [`UnreadableNote`](./UnreadableNote.md) — Ligne « n éléments ne sont pas lisibles par cette version » sous une liste partielle.
- [`SettingRow`](./SettingRow.md) — Ligne de réglage.
- [`ComingSoonPanel`](./ComingSoonPanel.md) — Carte « Bientôt disponible ».
- [`ColorSwatches`](./ColorSwatches.md) — Palette des 8 couleurs de serveur.
- [`StepTrail`](./StepTrail.md) — Fil des étapes de l'assistant.
- [`FingerprintBlock`](./FingerprintBlock.md) — Empreinte en 8 groupes de 4.
- [`BridgeDownBanner`](./BridgeDownBanner.md) — Liste des serveurs illisible, avec « Réessayer ».
- [`LevelBadge`](./LevelBadge.md) — Marque d'alerte : icône + « Attention » ou « Critique ».
- [`Gauge`](./Gauge.md) — Jauge : arc, valeur, libellé, marque d'alerte.
- [`TimeSeriesChart`](./TimeSeriesChart.md) — Courbe sur la fenêtre choisie, « Depuis N min » si l'historique est court.
- [`StatRow`](./StatRow.md) — Ligne « libellé : valeur » d'une carte.
- [`DashCard`](./DashCard.md) — Carte du tableau de bord (titre de section, corps).
- [`CoreBars`](./CoreBars.md) — Une barre par cœur du processeur.
- [`ResourceSlider`](./ResourceSlider.md) — Un curseur discret de mémoire ou de processeur et sa saisie exacte (HRT-146).
- [`DeployButton`](./DeployButton.md) — Le bouton « Déployer un service » : absent, désactivé et expliqué, ou actif (HRT-69).
- [`EnvVarRow`](./EnvVarRow.md) — Une variable du formulaire de déploiement : valeur masquée d'office pour un secret (HRT-69).
- [`PortMappingRow`](./PortMappingRow.md) — Une correspondance de ports : erreur, avertissement, port libre proposé par l'agent (HRT-69).
- [`ProgressSteps`](./ProgressSteps.md) — Les étapes d'une opération avec la barre de celle qui progresse (HRT-69).

## Organismes

- [`AuditFilters`](./AuditFilters.md) — Filtres du journal (recherche, comptes, types, résultats, période).
- [`AuditTable`](./AuditTable.md) — Tableau du journal : grille accessible, virtualisée, rafales regroupées.
- [`ServerRail`](./ServerRail.md) — Barre des serveurs.
- [`ServerNav`](./ServerNav.md) — Navigation du serveur.
- [`AppHeader`](./AppHeader.md) — En-tête avec pastille du lien.
- [`OfflineBanner`](./OfflineBanner.md) — Bandeau hors ligne.
- [`UpdateBanner`](./UpdateBanner.md) — Bandeau de mise à jour du client (annonce, téléchargement, installation, échec).
- [`UpdatePanel`](./UpdatePanel.md) — Section « Mises à jour » des réglages.
- [`LoginForm`](./LoginForm.md) — Formulaire de connexion (identifiant, mot de passe, se souvenir).
- [`AddServerWizard`](./AddServerWizard.md) — Assistant d'ajout en 3 temps.
- [`FingerprintAlert`](./FingerprintAlert.md) — Alerte bloquante d'identité changée.
- [`ReconnectPanel`](./ReconnectPanel.md) — Formulaire de reconnexion d'un serveur sans session.
- [`ServerEditForm`](./ServerEditForm.md) — Modification d'un serveur (nom, couleur, adresse).
- [`ServerRow`](./ServerRow.md) — Ligne du carnet de serveurs.
- [`DevLinkPanel`](./DevLinkPanel.md) — Panneau de simulation (développement seulement).
- [`DevActionPanel`](./DevActionPanel.md) — Bouton d'action de développement (`useServerAction`, `needs-link`).
- [`AccountTable`](./AccountTable.md) — Tableau des comptes et actions de ligne.
- [`AccountDetailDialog`](./AccountDetailDialog.md) — Détail d'un compte en lecture seule (droit de gérer les services).
- [`ServiceRightReason`](./ServiceRightReason.md) — Raison et commande sous un bouton de service désactivé faute du droit.
- [`ServicesRightRow`](./ServicesRightRow.md) — Ligne « Gestion des services » du détail d'un compte.
- [`SecurityAlertBanner`](./SecurityAlertBanner.md) — Bandeau « Attaque probable détectée ».
- [`AttackModeBanner`](./AttackModeBanner.md) — Bandeau « Mode attaque actif » (et suspendu).
- [`AttackModePanel`](./AttackModePanel.md) — Carte « Mode attaque » : état, bouton, raison d'indisponibilité.
- [`AdminActDialog`](./AdminActDialog.md) — LA fenêtre de confirmation de tous les actes d'administration (mot de passe selon ce que l'agent annonce, délai de 5 minutes, poste sans clé).
- [`ReauthField`](./ReauthField.md) — Le champ « Ton mot de passe » de la confirmation d'un acte.
- [`ReauthSettingCard`](./ReauthSettingCard.md) — Ligne « Demander mon mot de passe » de la page Sécurité.
- [`AttackModeDialog`](./AttackModeDialog.md) — Confirmation avec mot de passe de l'activation ou de la désactivation.
- [`TrustedDeviceTable`](./TrustedDeviceTable.md) — « Tes postes de confiance » : liste, « Ce poste », « Retirer », états.
- [`RemoveDeviceDialog`](./RemoveDeviceDialog.md) — Retrait d'un poste de confiance (mot de passe demandé).
- [`CreateAccountDialog`](./CreateAccountDialog.md) — Création d'un compte (validation en direct).
- [`PasswordDialog`](./PasswordDialog.md) — Changement de mot de passe (le sien ou celui d'un autre compte).
- [`OwnAccountCard`](./OwnAccountCard.md) — « Mon compte » d'un serveur dans les réglages.
- [`AgentUpdateCard`](./AgentUpdateCard.md) — « État du serveur : {nom} » : versions, mise à jour de l'agent, étapes, résultat.
- [`OperationCard`](./OperationCard.md) — La carte d'une opération de service : étape, progression, qui l'a lancée, bouton de la décision.
- [`OperationsStrip`](./OperationsStrip.md) — La bande « Opérations en cours » de la section Services.
- [`LifecycleButtons`](./LifecycleButtons.md) — Démarrer, arrêter, redémarrer : les gestes de l'état, la raison écrite dessous.
- [`ServiceLifecycleDialog`](./ServiceLifecycleDialog.md) — La fenêtre de confirmation (effet, avertissements, socle) des trois gestes.
- [`DeployForm`](./DeployForm.md) — Le formulaire « Déployer un service » : Général, variables, ports, commande, ressources, pied (HRT-69).
- [`EnvVarRows`](./EnvVarRows.md) — La section Variables d'environnement du formulaire de déploiement (HRT-69).
- [`PortMappingRows`](./PortMappingRows.md) — La section Ports du formulaire de déploiement (HRT-69).
- [`DeploySummaryDialog`](./DeploySummaryDialog.md) — Le récapitulatif « Vérifiez avant de lancer » et ses points d'attention (HRT-69).
- [`DeployProgressDialog`](./DeployProgressDialog.md) — La progression d'un déploiement, l'annulation et les issues (HRT-69).
- [`ServiceRowMenu`](./ServiceRowMenu.md) — Le menu d'actions d'une ligne de la liste des services.
- [`OperationDecisionDialog`](./OperationDecisionDialog.md) — La fenêtre de décision d'une opération (acte confirmé, mot de passe à chaque réponse).
- [`AgentUpdateSteps`](./AgentUpdateSteps.md) — Les étapes d'une mise à jour de l'agent (téléchargement, vérification, installation, redémarrage, contrôle).
- [`MachineCard`](./MachineCard.md) — Sections Machine et Durée de fonctionnement.
- [`CpuCard`](./CpuCard.md) — Section Processeur (jauge, courbe, cœurs).
- [`MemoryCard`](./MemoryCard.md) — Section Mémoire.
- [`GpuCard`](./GpuCard.md) — Section Carte graphique (ou « Non disponible sur cette machine »).
- [`GpuPanel`](./GpuPanel.md) — Une carte graphique (charge, mémoire vidéo, température).
- [`NetworkCard`](./NetworkCard.md) — Section Réseau (montant, descendant).
- [`DisksCard`](./DisksCard.md) — Section Disques (suit les montages).
- [`TemperaturesCard`](./TemperaturesCard.md) — Section Températures (ou explication sans sonde).
- [`ResourceSliders`](./ResourceSliders.md) — Mémoire et processeur à la création d'un service, bornes de la machine, jamais « sans limite » (HRT-146).
- [`FicheResourcesCard`](./FicheResourcesCard.md) — Processeur, mémoire et disque d'une fiche contre la machine, chaque chiffre avec sa preuve (HRT-102).
- [`FicheSheet`](./FicheSheet.md) — Les cartes de lecture de la fiche d'un jeu (HRT-102).
- [`ImposedDialog`](./ImposedDialog.md) — Ce que la fiche impose, réglage par réglage (HRT-102).
- [`AgreementDialog`](./AgreementDialog.md) — La fenêtre du contrat de licence, « Je refuse » et « J'accepte » (HRT-102).
- [`FicheSummaryDialog`](./FicheSummaryDialog.md) — « Vérifiez avant de lancer » d'une installation depuis une fiche (HRT-102).
- [`InstallAnswers`](./InstallAnswers.md) — Les « Réponses » de l'installation (HRT-102).
- [`InstallResources`](./InstallResources.md) — Les « Ressources » de l'installation (HRT-102).
- [`FirstStartPanel`](./FirstStartPanel.md) — Le premier démarrage long : des faits, jamais un pourcentage (HRT-102).

## Gabarits

- [`ServerLayout`](./ServerLayout.md) — Gabarit d'un serveur : navigation, en-tête, bandeau, page.

## Pages

- [`Welcome`](./Welcome.md) — Accueil (aucun serveur).
- [`AddServer`](./AddServer.md) — Ajout d'un serveur (assistant).
- [`Servers`](./Servers.md) — Carnet de serveurs.
- [`Dashboard`](./Dashboard.md) — Tableau de bord : la machine en direct (jauges, courbes, seuils, matériel absent).
- [`Accounts`](./Accounts.md) — Comptes (administrateurs) : liste, création, rôle, mots de passe, sessions, suppression.
- [`Security`](./Security.md) — Sécurité : « Mode attaque » (carte) et tes postes de confiance (liste, retrait avec mot de passe).
- [`Audit`](./Audit.md) — Journal d'activité (à venir).
- [`DeployService`](./DeployService.md) — « Déployer un service » : formulaire en page, récapitulatif, progression, abandon (HRT-69).
- [`Catalogue`](./Catalogue.md) — Les serveurs de jeu que l'agent propose (HRT-102).
- [`FicheDetail`](./FicheDetail.md) — La fiche d'un jeu et ses deux boutons d'installation (HRT-102).
- [`InstallFromFiche`](./InstallFromFiche.md) — Installer un serveur de jeu depuis une fiche en trois étapes (HRT-102).
- [`Settings`](./Settings.md) — Réglages.

## Règle partagée

- [`needs-link`](./needs-link.md) — prop `needsLink` et `useNeedsLink` : désactive et explique toute action qui exige le lien ou un rôle (unique source de vérité).

## À venir (autres tickets)

`Sparkline` (non retenu : les courbes de chaque carte suffisent), `AuditTable`, `AuditFilters`, `AgentUpdateSteps`.

Fichiers de l'interface : `apps/desktop/src/` (`components/{atoms,molecules,organisms}`, `layouts/`, `pages/`, `composables/`, `stores/`, `link/`, `errors/`, `router/`, `i18n/`, `styles/`). Pont Tauri typé : `src/bindings.ts` (généré, ne pas éditer).

## Conventions

- Tous textes → module `src/i18n/fr.ts` + `t(clé, { paramètres })` (clés par chemin, `welcome.title`). Le raccordement à `vue-translate` est repoussé (voir ADR-0010).
- Aucun texte en dur. Aucun `style=` en ligne : une valeur visuelle = un jeton de `src/styles/tokens.css` (la couleur d'un serveur est un numéro de palette).
- Désactivé = `aria-disabled` (jamais `disabled` natif : le focus clavier est conservé), explication par `HTooltip`. Pour le lien et le rôle : la prop `needsLink`, nulle part ailleurs.
- Aucune valeur visuelle littérale : `npm run lint` échoue sur une couleur ou un `px` hors de `tokens.css`.
- Responsive : captures 1366, 1920 et 2560 px (`npm run e2e`, `apps/desktop/e2e/screenshots/`, non commitées).
- [LogViewer](./LogViewer.md), [LogMarker](./LogMarker.md), [LogSearch](./LogSearch.md) : onglet Journaux d'un service (HRT-67)
- [ServiceDetail](./ServiceDetail.md), [EnvVarsSection](./EnvVarsSection.md), [HTabs](./HTabs.md), [BoundedText](./BoundedText.md) : détail d un service (HRT-63)
