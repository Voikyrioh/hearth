---
id: ADR-0026
titre: Case « Lancer Hearth au démarrage de Windows » dans l'installateur : rappels SHOW et LEAVE de la page d'accueil, via les crochets du modèle de Tauri
type: architecture
statut: acceptée
date: 2026-10-07
portee: projet
remplace: —
liens: [ADR-0002, ADR-0010, ADR-0017, BR-CLIENT-006, BR-CLIENT-007, BR-CLIENT-008, BR-CLIENT-010, HRT-21]
---

# ADR-0026 : Case de démarrage dans l'installateur NSIS

## Contexte

La spec veut que l'installateur propose le lancement au démarrage de Windows, décoché par défaut (BR-CLIENT-006). Le réglage existe déjà dans l'application : le greffon `tauri-plugin-autostart` écrit la valeur `Hearth` de `HKCU\...\Run` (argument `--minimized`) et la relit ; c'est la seule source de vérité. L'installateur NSIS est celui du modèle de Tauri (`installerHooks` = `installer/hooks.nsh`). Contraintes : l'installation silencieuse (`/S`) garde le défaut ; la mise à jour automatique lance l'installateur en passif (`/UPDATE /P`, ADR-0017) et ne doit jamais écraser le choix fait dans l'application (BR-CLIENT-008).

## Décision

1. **La case est ajoutée à la page d'accueil du modèle** par les rappels `MUI_PAGE_CUSTOMFUNCTION_SHOW` et `_LEAVE` définis dans `hooks.nsh` (le fichier est inclus avant les pages : une `Page custom` s'y placerait avant l'accueil, et le modèle ne donne pas d'autre crochet de page). C'est le geste que le modèle fait lui-même pour la case « Tout effacer » de la désinstallation. Le contrôle de texte du modèle est laissé vide et le texte d'accueil (tutoiement, court) est redessiné avec la case et l'aide, sans recouvrement (voir Conséquences).
2. **Aucun stockage nouveau** : l'installateur écrit (case cochée) ou retire (décochée) la valeur `Run` que lit le réglage de l'application, dans le crochet `NSIS_HOOK_POSTINSTALL`, seulement si la page a été franchie (`LEAVE` le consigne). Même valeur que le greffon (nom du produit, `chemin --minimized`, sans guillemets) et la valeur « activé » du Gestionnaire des tâches comme le greffon.
3. **Silencieux et mise à jour : aucune écriture.** `/S` n'affiche aucune page ; le mode passif saute l'accueil (rappel PRE du modèle, que nous ne redéfinissons pas). La variable « page franchie » reste vide et le crochet ne fait rien : défaut désactivé sur une installation neuve, choix de l'utilisateur intact sur une mise à jour.
4. **Réinstallation à la main** : la case est cochée si le démarrage est activé (lu à l'ouverture de l'interface, avant la page de réinstallation qui peut désinstaller l'ancienne version et sa valeur) ; l'utilisateur confirme ou change.
5. **Désinstallation** : inchangée, le modèle retire la valeur hors mise à jour (BR-CLIENT-010).

## Alternatives écartées

- **Gabarit complet** (`bundle.windows.nsis.template`) : permet une vraie page, mais recopie ~1000 lignes du modèle, à rebaser à chaque version de l'outil (corrections de sécurité de l'installateur comprises). Trop intrusif pour une case.
- **Case sur la page de fin** : l'installation est déjà faite, et la page est sautée en silencieux ; le modèle y occupe déjà ses deux cases.
- **Option de ligne de commande** (`/AUTOSTART`) : pas demandée ; ajoutable plus tard sans changer cette décision.
- **Second réglage lu par l'application** (fichier, clé propre) : une deuxième source de vérité, rejetée (BR-CLIENT-006).

## Conséquences

- Le script dépend du modèle NSIS embarqué dans `@tauri-apps/cli`, **relu à la version 2.12.1** (ordre des pages, `SkipIfPassive`, crochets) ; un test épingle cette version du verrou et force une relecture à chaque montée. Si une version de Tauri définit elle-même un rappel SHOW/LEAVE ou `MUI_WELCOMEPAGE_TEXT` avant l'accueil, la compilation NSIS échoue (redéfinition) : le job `desktop` de la CI construit l'installateur à chaque PR.
- **Mise en page** (revue Stephen r1) : la page d'accueil de MUI2 crée son texte en `120u 45u 195u 130u`, fond opaque. Raccourcir la phrase ne raccourcit pas le contrôle ; le script laisse donc ce texte VIDE (`MUI_WELCOMEPAGE_TEXT " "`) et redessine le texte d'accueil, la case et l'aide en trois rectangles disjoints (un test les vérifie, en unités de boîte de dialogue : même rendu à 100, 125 et 150 %). Le rendu est capturé en image sur le runner jetable de la CI (`installer-ci.ps1`, artefact `hearth-installer-evidence`).
- **Même lecture que le greffon** : « démarrage activé » = entrée `Run` présente ET non désactivée dans le Gestionnaire des tâches (`StartupApproved\Run` : moins de 8 octets ou valeur absente = activée, sinon les 8 derniers octets doivent être nuls), comme `auto-launch` 0.6.0 (`is_enabled`). La case arrive cochée seulement dans ce cas ; décochée, l'entrée n'est retirée que si le démarrage était activé ; cochée, la valeur est celle du greffon (`chemin --minimized`, SANS guillemets : un profil dont le chemin a une espace donne un chemin ambigu, défaut du greffon antérieur à cette PR, à suivre) et la valeur « activé » est écrite dans `StartupApproved`.
- **Installation pour tous les utilisateurs impossible** avec `installMode: currentUser` (ni page de choix, ni élévation). Si la configuration passait à `both` ou `perMachine`, le crochet écrirait dans la ruche de l'utilisateur qui élève, pas de celui qui se connecte : cette décision serait à reprendre.
- Le geste est proche de celui du modèle pour la case de la désinstallation, sans lui être identique : le modèle crée la sienne par `CreateWindowEx` sur une page qui n'est pas `nsDialogs`, à un endroit vide ; ici on ajoute des contrôles `nsDialogs` à la page d'accueil, qui l'est.
- Le nom de valeur est répété dans `hooks.nsh` (`PRODUCTNAME` n'est défini qu'après son inclusion) ; un garde de compilation et un test (`tests/installer.rs`) le comparent au nom du produit.
- Preuve : tests de texte (`tests/installer.rs`) PLUS exécution réelle de l'installateur sur le runner Windows jetable de la CI (`apps/desktop/scripts/installer-ci.ps1`, étape du job `desktop`) : page d'accueil capturée, états initiaux de la case, `/S`, réinstallation, `/UPDATE /P`, parcours complet, désinstallation. Jamais sur un poste de travail. Reste à la main (runbook, point 8) : les échelles d'affichage 125 et 150 %.
