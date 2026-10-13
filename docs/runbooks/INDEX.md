# Runbooks — Hearth

Procédures opérationnelles : installation, diagnostics, recovery.

- [Installer, réinstaller et désinstaller l'agent](./installer-agent.md) — une commande, interactive ou sans question, installation gérée, refus et dépannage
- [Mettre l'agent à jour à distance](./mettre-a-jour-agent.md) — clé de signature, publier une version, lancer, suivre, retour automatique, reprise à la main
- [Publier une version du client](./publier-une-version-du-client.md) — clés de publication (en place, rotation), environnement `release`, flux `publish-client` (à la main, rend un artefact signé, sans release), dépôt dans le bucket, release GitHub sans binaire, premier essai (HRT-56), dépannage
- [Accorder le droit à un compte](./accorder-le-droit-de-gerer-les-services.md) — le droit « gérer les services » se donne depuis le serveur : question à l'installation (`--services`, `--no-services`, `HEARTH_SERVICES`), commande `account services {compte} --grant ou --revoke`, comptes existants après une mise à jour
- [Récupérer l'accès administrateur](./recuperer-acces-administrateur.md) — aucun mot de passe administrateur connu, ou plus aucun compte
- [Sortir du mode attaque depuis le serveur](./sortir-du-mode-attaque.md) — `hearth-agent attack-mode status|off`, qui peut le lancer (qui lit le dossier de données), autres sorties
- [Le secret d'empreinte des requêtes suivies](./secret-d-empreinte.md) — où il est, perdu ou remplacé, démarrage refusé (ADR-0034)
- [Vérifier les services sur la forge](./verifier-les-services-sur-forge.md) — squelette (HRT-61) : moteur détecté et liste en lecture seule, règles de sécurité (jamais Terraria sans sauvegarde), parties à venir

Structure : `{titre-court}.md` avec sections (prérequis, étapes, diagnostic, rollback).

Exemples (futurs) :
- Installer client Windows (NSIS, coffre, Tauri)
- Récupérer certificat agent
- Diagnostiquer coupure réseau
