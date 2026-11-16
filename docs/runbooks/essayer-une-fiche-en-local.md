# Essayer une fiche de jeu sur Docker Desktop

Procédure (HRT-226, ADR-0053) pour lancer le banc d'essai réel `tests/fiche_live_game.rs` sur un poste Windows qui a Docker Desktop, sans CI (quota Actions) ni serveur. Le banc passe par le chemin de l'agent (traduction, jugement du déploiement ordinaire, plan fermé, création par `BollardEngine`, donc sous `no-new-privileges`) sur la vraie image épinglée.

## Risque

Monter `/var/run/docker.sock` et lancer en `--network host` donne `root` sur la machine virtuelle de Docker Desktop (et un accès aux autres conteneurs du poste). Cette procédure est RÉSERVÉE à l'essai local manuel sur le poste du propriétaire (ADR-0053, décision 1) ; elle ne se lance jamais sur un runner partagé ni sur un serveur, et le conteneur de test n'exécute que le code du dépôt relu.

## Pourquoi un conteneur de test

`hearth-agent` ne compile les tests Unix que sous Linux, et le socket du moteur est un socket Unix. Le test tourne donc dans un conteneur Linux `rust:1.95` qui monte le socket de Docker Desktop (`/var/run/docker.sock`). Deux pièges :

- **Les dossiers gérés doivent exister au même chemin pour le conteneur de test ET pour le moteur** : un bind-mount est résolu par le moteur (la machine virtuelle de Docker Desktop), pas par le conteneur de test. Utiliser un volume nommé monté sur son propre chemin (`-v hl-data:/var/lib/docker/volumes/hl-data/_data`) et poser `HEARTH_FICHE_LIVE_OUT` dessous. `/tmp` de la machine virtuelle est un tmpfs de moins de 1 Gio : ne jamais s'en servir.
- `--network host` : le test se connecte aux ports publiés par le moteur sur `127.0.0.1`.

## Lancer

```sh
docker volume create hl-data
MSYS_NO_PATHCONV=1 docker run --rm --network host \
  -v "<repo>:/src" -v hearth-cat-target:/target -v hearth-cat-cargo:/usr/local/cargo/registry \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v hl-data:/var/lib/docker/volumes/hl-data/_data \
  -e CARGO_TARGET_DIR=/target -e HEARTH_TEST_FICHE_LIVE=1 \
  -e HEARTH_FICHE_LIVE_OUT=/var/lib/docker/volumes/hl-data/_data/out -e HEARTH_LIVE_GAME=<id> \
  -w /src <image Linux avec rust 1.95 et la CLI docker> \
  cargo test -p hearth-agent --test fiche_live_game -- --ignored --nocapture --test-threads=1
```

L'image de test est `rust@sha256:6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1` (`rust:1.95-bookworm` au 16/11/2026, à tirer par cette empreinte) plus la CLI `docker` copiée depuis `docker@sha256:851f91d241214e7c6db86513b270d58776379aacc5eb9c4a87e5b47115e3065c` (`docker:27-cli` au 16/11/2026, tirée par cette empreinte). Variables : `HEARTH_LIVE_GAME` (entrée du tableau `GAMES`), `HEARTH_LIVE_SCENARIOS` (par exemple `min`), `HEARTH_LIVE_MEM_GIB` (limite du premier scénario), `HEARTH_LIVE_PRESEED` (dossier contenant un sous-dossier par identifiant de dossier de la fiche, déplacé à la place du dossier géré pour éviter de retélécharger un gros jeu). Le relevé est écrit dans `<out>/<id>/evidence.toml` ; le copier à côté de la fiche, avec `golden.toml` (`docker image inspect` de l'image épinglée).

## Limites de la machine

- Mémoire de la machine virtuelle : 8 Gio par défaut. Une limite de 6 Gio est le plus qu'un scénario peut demander.
- Disque de la machine virtuelle : surveiller `df` du volume. Les images et les jeux téléchargés s'y accumulent ; supprimer l'image de chaque jeu après la mesure (`docker rmi <image>` précis, jamais `docker image prune -a`).
- Un conteneur sans nom lancé par erreur (`docker run image commande` sur une image à point d'entrée fixe) reste en marche : `docker ps` avant de conclure.

## Lire un résultat

Un scénario passe si le signe « prêt » vient, si les ports publiés écoutent (TCP : connexion ; UDP : socket en écoute lu dans le conteneur), si le réglage posé est pris en compte, si l'arrêt par le signal de l'image sort en code 0 avant le délai en réécrivant des fichiers du dossier du monde, si le service reste arrêté et si le redémarrage retrouve le monde. Les chiffres de mémoire sont ceux d'un serveur vide sous la limite essayée : `min_bytes` et `recommended_bytes` d'une fiche sont des limites essayées, jamais une mesure sous charge.
