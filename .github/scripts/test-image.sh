#!/usr/bin/env bash
# Images des tests du vrai moteur (HRT-168). Appelé par l'action `.github/actions/test-image`, elle-même
# utilisée par les jobs `engine` et `engine-logs-long` de ci.yml.
#
# Deux images, toutes deux fixées PAR EMPREINTE (le démon vérifie l'empreinte au téléchargement : ECR et
# Docker Hub servent donc forcément les mêmes octets, ou le téléchargement est refusé) :
#   - busybox:1.36, l'image des tests (`IMAGE` de tests/engine_real.rs et engine_supervised.rs) ;
#   - registry:2, un registre LOCAL (localhost:5000) alimenté depuis l'archive, d'où le moteur récupère
#     l'image pendant la suite de contrat (`HEARTH_TEST_PULL_REPO=localhost:5000`) : aucun quota public.
# Pour chaque image : démon, sinon archive `docker save` du dossier de cache, sinon téléchargement par
# empreinte chez ECR puis, en repli, chez Docker Hub. Un registre en échec est dit comme tel
# (« REGISTRE ... » avec sa dernière erreur réelle), jamais comme un défaut du code testé.
# Usage : test-image.sh <dossier-du-cache>. Ne change qu'avec l'image : le cache a pour clé le hash de ce fichier.
set -euo pipefail

DIR="${1:?dossier du cache}"
SOURCES=("public.ecr.aws/docker/library" "docker.io/library")
# nom local | dépôt | empreinte de l'index multi-architecture (relevée le 2026-10-24)
IMAGES=(
  "public.ecr.aws/docker/library/busybox:1.36|busybox|sha256:73aaf090f3d85aa34ee199857f03fa3a95c8ede2ffd4cc2cdb5b94e566b11662"
  "public.ecr.aws/docker/library/registry:2|registry|sha256:a3d8aaa63ed8681a604f1dea0aa03f100d5895b6a58ace528858a7b332415373"
)
TRANSIENT='toomanyrequests|too many requests|rate exceeded|rate limit|bad gateway|service unavailable|gateway timeout|tls handshake timeout|i/o timeout|context deadline exceeded|connection reset|connection refused|unexpected eof|temporary failure in name resolution'

provide() {
  local tag="$1" repo="$2" digest="$3" file="$DIR/$2.tar"
  if docker image inspect "$tag" >/dev/null 2>&1; then echo "$repo : déjà présente"; return 0; fi
  if [ -s "$file" ]; then
    if load_err="$(docker load --input "$file" 2>&1)" && docker image inspect "$tag" >/dev/null 2>&1; then
      echo "$repo : chargée depuis le cache de la CI"; return 0
    fi
    echo "::warning::$repo : archive du cache inutilisable ($(printf '%s' "$load_err" | tr '\n' ' ')), retéléchargement"
    rm -f "$file"
  fi
  local first=1 src ref wait attempt out last
  for src in "${SOURCES[@]}"; do
    ref="$src/$repo@$digest"
    if [ "$first" -eq 0 ]; then echo "::notice::$repo : repli sur $src (empreinte $digest)"; fi
    first=0
    wait=5; last=""
    for attempt in 1 2 3; do
      if out="$(docker pull --quiet "$ref" 2>&1)"; then
        docker tag "$ref" "$tag"
        mkdir -p "$DIR"
        # Écrite à côté puis renommée : une archive vide ou coupée ne part jamais dans le cache.
        if docker save --output "$file.part" "$tag" && [ -s "$file.part" ]; then
          mv "$file.part" "$file"
        else
          rm -f "$file.part"
          echo "::warning::$repo : archive non écrite (le cache ne sera pas rempli)"
        fi
        echo "$repo : téléchargée depuis $src par empreinte (essai $attempt)"
        return 0
      fi
      last="$out"
      echo "REGISTRE $src, $repo, essai $attempt/3 : $out"
      # Seule une limite de débit ou une panne passagère justifie d'attendre et de réessayer ici ; sinon
      # (quota de données épuisé, image inconnue, démon KO...) on sort de cette boucle, donc on passe à la source suivante.
      if printf '%s' "$out" | grep -Eiq 'data limit exceeded'; then break; fi
      if ! printf '%s' "$out" | grep -Eiq "$TRANSIENT"; then break; fi
      if [ "$attempt" -lt 3 ]; then sleep "$wait"; wait=$((wait * 2)); fi
    done
    echo "::warning::REGISTRE $src n'a pas fourni $repo. Dernière erreur : $(printf '%s' "$last" | tr '\n' ' ')"
    ERRORS+="$src : $(printf '%s' "$last" | tr '\n' ' ') | "
  done
  echo "::error::REGISTRE : $repo ($digest) introuvable chez ECR et Docker Hub et absente du cache. Erreurs : $ERRORS"
  return 1
}

ERRORS=""
for entry in "${IMAGES[@]}"; do
  IFS='|' read -r tag repo digest <<<"$entry"
  ERRORS=""
  provide "$tag" "$repo" "$digest"
done

# Registre local pour la suite de contrat : le moteur y récupère busybox sans toucher au réseau public.
if ! docker ps --format '{{.Names}}' | grep -qx hearth-ci-registry; then
  docker rm -f hearth-ci-registry >/dev/null 2>&1 || true
  docker run -d --name hearth-ci-registry -p 127.0.0.1:5000:5000 "public.ecr.aws/docker/library/registry:2" >/dev/null
fi
for _ in $(seq 1 30); do
  if curl -fsS http://localhost:5000/v2/ >/dev/null 2>&1; then break; fi
  sleep 1
done
curl -fsS http://localhost:5000/v2/ >/dev/null || { echo "::error::registre local localhost:5000 injoignable (défaut de la préparation, pas du code testé)"; exit 1; }
docker tag "public.ecr.aws/docker/library/busybox:1.36" localhost:5000/busybox:1.36
docker push --quiet localhost:5000/busybox:1.36 >/dev/null
echo "registre local prêt : localhost:5000/busybox:1.36"
