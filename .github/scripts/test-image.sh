#!/usr/bin/env bash
# Image des tests du vrai moteur (HRT-168). Appelé par les jobs `engine` et `engine-logs-long` de ci.yml.
#
# Ordre : 1. déjà présente dans le démon, 2. archive `docker save` restaurée du cache de la CI
# (actions/cache, clé = image + empreinte), 3. téléchargée PAR EMPREINTE depuis ECR, puis en repli
# depuis Docker Hub (même empreinte, donc mêmes octets), puis sauvée dans l'archive pour le cache.
# Un registre en panne ou à quota épuisé est dit comme tel (« REGISTRE : ... »), jamais comme un
# défaut du code testé. Valeurs jumelles : `IMAGE` de `tests/engine_real.rs` et `engine_supervised.rs`.
set -u

TAG="public.ecr.aws/docker/library/busybox:1.36"
# Empreinte de l'index multi-architecture de busybox:1.36, identique chez ECR et Docker Hub (relevée
# avec `docker buildx imagetools inspect` le 2026-10-24). Ne change qu'avec l'image des tests.
DIGEST="sha256:73aaf090f3d85aa34ee199857f03fa3a95c8ede2ffd4cc2cdb5b94e566b11662"
SOURCES=("public.ecr.aws/docker/library/busybox" "docker.io/library/busybox")
ARCHIVE="${1:?chemin du fichier archive}"

if docker image inspect "$TAG" >/dev/null 2>&1; then echo "image déjà présente"; exit 0; fi

if [ -s "$ARCHIVE" ]; then
  if docker load --input "$ARCHIVE" >/dev/null 2>&1 && docker image inspect "$TAG" >/dev/null 2>&1; then
    echo "image chargée depuis le cache de la CI ($ARCHIVE)"; exit 0
  fi
  echo "::warning::archive du cache illisible, retéléchargement"
fi

transient='toomanyrequests|too many requests|rate exceeded|rate limit|data limit exceeded|bad gateway|service unavailable|gateway timeout|tls handshake timeout|i/o timeout|context deadline exceeded|connection reset|connection refused|unexpected eof|temporary failure in name resolution'

for src in "${SOURCES[@]}"; do
  wait=5
  for attempt in 1 2 3; do
    if out="$(docker pull --quiet "$src@$DIGEST" 2>&1)"; then
      docker tag "$src@$DIGEST" "$TAG" || exit 1
      mkdir -p "$(dirname "$ARCHIVE")"
      docker save --output "$ARCHIVE" "$TAG" || { rm -f "$ARCHIVE"; echo "::warning::archive non écrite (le cache ne sera pas rempli)"; }
      echo "image téléchargée depuis $src par empreinte $DIGEST (essai $attempt)"
      exit 0
    fi
    echo "REGISTRE $src, essai $attempt/3 : $out"
    # Quota épuisé ou panne : inutile de patienter sur ce registre, on passe au suivant.
    if printf '%s' "$out" | grep -Eiq 'data limit exceeded'; then break; fi
    if ! printf '%s' "$out" | grep -Eiq "$transient"; then break; fi
    if [ "$attempt" -lt 3 ]; then sleep "$wait"; wait=$((wait * 2)); fi
  done
  echo "::warning::REGISTRE : $src n'a pas fourni l'image des tests, repli sur le suivant"
done
echo "::error::REGISTRE : image des tests ($TAG, $DIGEST) introuvable chez ECR et Docker Hub et absente du cache (quota ou panne des registres, pas un défaut du code testé)"
exit 1
