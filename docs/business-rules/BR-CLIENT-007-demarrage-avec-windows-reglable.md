---
id: BR-CLIENT-007
domaine: CLIENT
titre: Le lancement au démarrage se règle depuis l'application
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-client.md (BR-CLIENT-007), HRT-08
maj: 2026-10-04
---

# BR-CLIENT-007 — Le lancement au démarrage se règle depuis l'application

## Règle
Réglages > Général > « Lancer Hearth au démarrage de Windows » active ou désactive l'entrée de démarrage de l'utilisateur ; le changement vaut dès la session Windows suivante. La valeur écrite est `"<exe>" --minimized`, entre guillemets (HRT-29, ADR-0028) ; au démarrage, une ancienne valeur sans guillemets est réécrite avec la même cible sans jamais changer le choix de l'utilisateur (absente reste absente, activée reste activée, désactivée dans le Gestionnaire des tâches reste désactivée). Code : `domain.rs::startup_command` / `migrated_run_value`, `startup.rs` (FIX:01M4B118DAFBQZYQX1E5ERY8CA). Chemin d'application trop long (la ligne dépasserait 260 caractères, limite de la clé `Run`) : l'activation est refusée par une erreur typée plus que d'écrire une valeur que Windows ignorerait. Valeur absente (clé ou valeur) = désactivé, sans erreur ; seule la ruche utilisateur est migrée. L'interface affiche l'état renvoyé par le cœur Rust, pas l'état espéré ; en cas d'échec le réglage reste inchangé et un message l'indique.

## Application (code)
- `apps/desktop/src-tauri/src/settings.rs::set_launch_at_startup` : `enable`/`disable` du greffon puis relecture.
- `apps/desktop/src-tauri/src/commands.rs::set_launch_at_startup` : commande typée exposée à l'interface.
- `apps/desktop/src/stores/settings.ts::setLaunchAtStartup` et `src/pages/Settings.vue`.

## Vérification
- Tests : `src/stores/settings.test.ts` (envoi `{ enabled }`, suivi de la valeur Rust, échec = ancienne valeur + message), `src/pages/Settings.test.ts`.

## Cas limites
- Pont Tauri absent (navigateur de revue) : message « Impossible de lire tes réglages. », interrupteur verrouillé.

## Règles liées
- BR-CLIENT-006

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
