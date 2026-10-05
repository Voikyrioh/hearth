# Runbooks — Hearth

Procédures opérationnelles : installation, diagnostics, recovery.

- [Installer, réinstaller et désinstaller l'agent](./installer-agent.md) — une commande, interactive ou sans question, installation gérée, refus et dépannage
- [Mettre l'agent à jour à distance](./mettre-a-jour-agent.md) — clé de signature, publier une version, lancer, suivre, retour automatique, reprise à la main
- [Publier une version du client](./publier-une-version-du-client.md) — paire de clés minisign, secrets du dépôt, flux `publish-client` (à la main, brouillon), premier essai, dépannage
- [Récupérer l'accès administrateur](./recuperer-acces-administrateur.md) — aucun mot de passe administrateur connu, ou plus aucun compte

Structure : `{titre-court}.md` avec sections (prérequis, étapes, diagnostic, rollback).

Exemples (futurs) :
- Installer client Windows (NSIS, coffre, Tauri)
- Récupérer certificat agent
- Diagnostiquer coupure réseau
