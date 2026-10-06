---
id: BR-CLIENT-006
domaine: CLIENT
titre: Le lancement au démarrage de Windows est proposé à l'installation, désactivé par défaut
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-client.md (BR-CLIENT-006), HRT-08
maj: 2026-10-07
---

# BR-CLIENT-006 — Le lancement au démarrage de Windows est proposé à l'installation, désactivé par défaut

**Application : appliquée** (HRT-21). La case est sur la page d'accueil de l'installateur ; le réglage de l'application (BR-CLIENT-007) lit la même entrée.

## Règle
Un client fraîchement installé ne se lance pas au démarrage de Windows. L'état réel est celui de l'entrée `Run` de l'utilisateur (greffon autostart), jamais un doublon dans les réglages. Au démarrage de Windows l'application est lancée avec `--minimized` et reste dans la zone de notification. L'installateur propose la case « Lancer Hearth au démarrage de Windows », décochée, avec son aide (« Hearth s'ouvre tout seul quand tu ouvres ta session, réduit près de l'horloge. Tu pourras changer ce choix à tout moment dans les réglages de l'application. »). Case cochée : il écrit l'entrée `Run` (`<dossier>\Hearth.exe --minimized`, sans guillemets, exactement comme le greffon, et « activé » dans le Gestionnaire des tâches) à la fin de la copie des fichiers ; au premier lancement, le réglage la montre donc activée, sans second stockage. Décochée : il n'écrit rien ; il retire l'entrée seulement si le démarrage était activé (une entrée désactivée à la main dans le Gestionnaire des tâches n'est pas touchée).

## Application (code)
- `apps/desktop/src-tauri/src/settings.rs::read` : lit l'état de l'entrée de démarrage (absente = faux).
- `apps/desktop/src-tauri/src/domain.rs::is_minimized_launch` : `--minimized` garde la fenêtre cachée.
- `apps/desktop/src-tauri/installer/hooks.nsh` : case ajoutée à la page d'accueil (rappels SHOW et LEAVE), écriture dans `NSIS_HOOK_POSTINSTALL` ; textes dans `installer/French.nsh` (voie : ADR-0026). Script NSIS, aucune fonction `domain/`.

## Vérification
- Test : `tests/domain.rs::minimized_launch_is_detected_from_the_startup_argument` ; `tests/settings.rs::startup_is_off_by_default_then_written_and_read_back`.
- Front : `src/stores/settings.test.ts::starts with the autostart option off`.
- Script de l'installateur : `tests/installer.rs` (case décochée par défaut, aucune écriture hors du bloc gardé par la page, même entrée que l'application, mise en page sans recouvrement, version de l'outil dont le modèle a été lu, textes) ; EN VRAI sur le runner jetable de la CI : `apps/desktop/scripts/installer-ci.ps1` (page capturée en artefact `hearth-installer-evidence`, `/S`, réinstallation, `/UPDATE /P`, parcours complet, désinstallation) ; `tests/settings.rs::an_entry_written_by_the_installer_shows_in_the_settings_and_can_be_turned_off`. Compilation du script : job `desktop` de la CI.
- À la main (jamais fait en test, il écrirait dans le registre) : voir le runbook `publier-une-version-du-client`, « Premier essai », point 8.

## Cas limites
- Installation silencieuse (`/S`) : pas de page, donc rien n'est écrit : défaut désactivé sur une installation neuve.
- Mise à jour automatique (`/UPDATE /P`, mode passif) : pas de page, rien n'est écrit : le choix fait dans l'application n'est JAMAIS écrasé (BR-CLIENT-008).
- Réinstallation à la main par-dessus : la case est cochée si le démarrage est activé (entrée présente ET non désactivée dans le Gestionnaire des tâches, comme le lit le greffon), l'utilisateur confirme ou change ; l'état est lu avant la page de réinstallation, qui peut désinstaller l'ancienne version et son entrée.
- Annuler l'installation à l'accueil : rien n'est écrit (le choix n'est retenu qu'en quittant la page).

## Règles liées
- BR-CLIENT-007, BR-CLIENT-008

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
- 2026-10-04 — revue Stephen : titre d'origine rétabli, statut « partiellement appliquée », suivi HRT-21.
- 2026-10-07 — case appliquée dans l'installateur (HRT-21).
