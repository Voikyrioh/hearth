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
# Usage : test-image.sh <dossier-du-cache>. Les empreintes sont dans `test-image.digests` : le cache a pour clé le hash de CE fichier-là.
set -euo pipefail

DIR="${1:?dossier du cache}"
SOURCES=("public.ecr.aws/docker/library" "docker.io/library")
# nom local | dépôt | empreinte : `test-image.digests` (son hachage est la clé du cache).
mapfile -t IMAGES < <(grep -v '^#' "$(dirname "$0")/test-image.digests" | grep -v '^$')
TRANSIENT='toomanyrequests|too many requests|rate exceeded|rate limit|bad gateway|service unavailable|gateway timeout|tls handshake timeout|i/o timeout|context deadline exceeded|connection reset|connection refused|unexpected eof|temporary failure in name resolution'

save_archive() {
  local tag="$1" file="$2"
  mkdir -p "$DIR"
  # Écrite à côté puis renommée : une archive vide ou coupée ne part jamais dans le cache.
  if docker save --output "$file.part" "$tag" && [ -s "$file.part" ]; then
    mv "$file.part" "$file"
  else
    rm -f "$file.part"
    echo "::warning::$tag : archive non écrite (le cache ne sera pas rempli)"
  fi
}

# Présente par l'EMPREINTE (RepoDigests), pas seulement par le tag. Une image rechargée d'une archive perd ses
# RepoDigests : elle n'est donc jamais prise pour présente ici, seulement par le chemin de l'archive ci-dessous
# (archive écrite par ce script après un téléchargement par empreinte, sous une clé qui suit ce fichier).
has_digest() {
  docker image inspect --format '{{range .RepoDigests}}{{println .}}{{end}}' "$1" 2>/dev/null | grep -q "@$2\$"
}

provide() {
  local tag="$1" repo="$2" digest="$3" file="$DIR/$2.tar" ERRORS=""
  if has_digest "$tag" "$digest"; then
    echo "$repo : déjà présente (empreinte vérifiée)"
    [ -s "$file" ] || save_archive "$tag" "$file"
    return 0
  fi
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
        save_archive "$tag" "$file"
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

for entry in "${IMAGES[@]}"; do
  IFS='|' read -r tag repo digest <<<"$entry"
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

# --- Couche propre ajoutée DIRECTEMENT dans le registre local, par son API HTTP -------------------------------
# Le démon ne voit jamais cette couche (ni son blob dans son magasin de contenu, qu'un `docker build` + `rmi`
# laisse en place avec le magasin containerd) : le « pull » du contrat télécharge donc forcément de vrais octets.
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
REG="http://localhost:5000"
REPO="busybox"
TAGNAME="1.36"
ACCEPT='application/vnd.docker.distribution.manifest.v2+json, application/vnd.oci.image.manifest.v1+json, application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json'

fail() { echo "::error::registre local, étape « $1 » : $2 (défaut de la préparation des tests, pas du code testé)"; exit 1; }

# http <étape> <code attendu> <fichier de sortie> <curl args...> : échoue en disant le code reçu.
http() {
  local step="$1" want="$2" out="$3"; shift 3
  local code
  code="$(curl -sS -o "$out" -w '%{http_code}' "$@")" || fail "$step" "curl a échoué"
  case " $want " in *" $code "*) ;; *) fail "$step" "code HTTP $code (attendu $want) : $(head -c 300 "$out" | tr '\n' ' ')";; esac
}

# push_blob <étape> <fichier> <digest> : POST puis PUT ?digest=.
push_blob() {
  local step="$1" file="$2" digest="$3" loc sep code
  curl -sS -D "$work/post.hdr" -o /dev/null -X POST "$REG/v2/$REPO/blobs/uploads/" || fail "$step" "curl a échoué (POST)"
  code="$(awk 'NR==1{print $2}' "$work/post.hdr")"
  [ "$code" = "202" ] || fail "$step" "code HTTP $code au POST (attendu 202)"
  loc="$(awk 'tolower($1)=="location:"{print $2}' "$work/post.hdr" | tr -d '\r')"
  [ -n "$loc" ] || fail "$step" "pas d'adresse d'envoi (Location) en réponse au POST"
  case "$loc" in http*) ;; *) loc="$REG$loc";; esac
  case "$loc" in *\?*) sep="&";; *) sep="?";; esac
  http "$step" "201" "$work/put.out" -X PUT -H 'Content-Type: application/octet-stream' --data-binary "@$file" "$loc${sep}digest=$digest"
}

# 1. L'image épinglée (inchangée) dans le registre local ; le démon n'en garde que son tag d'origine.
docker tag "public.ecr.aws/docker/library/busybox:1.36" "localhost:5000/$REPO:$TAGNAME"
docker push --quiet "localhost:5000/$REPO:$TAGNAME" >/dev/null
docker rmi "localhost:5000/$REPO:$TAGNAME" >/dev/null

# 2. Son manifeste (celui de linux/amd64 si c'est un index) et sa config.
http "lire le manifeste" "200" "$work/m0.json" -D "$work/m0.hdr" -H "Accept: $ACCEPT" "$REG/v2/$REPO/manifests/$TAGNAME"
kind="$(awk 'tolower($1)=="content-type:"{print $2}' "$work/m0.hdr" | tr -d '\r;')"
case "$kind" in
  *index*|*manifest.list*)
    sub="$(jq -r '[.manifests[] | select(.platform.os=="linux" and .platform.architecture=="amd64")][0].digest // empty' "$work/m0.json")"
    [ -n "$sub" ] || fail "choisir linux/amd64" "aucun manifeste linux/amd64 dans l'index"
    http "lire le manifeste amd64" "200" "$work/m.json" -D "$work/m.hdr" -H "Accept: $ACCEPT" "$REG/v2/$REPO/manifests/$sub"
    kind="$(awk 'tolower($1)=="content-type:"{print $2}' "$work/m.hdr" | tr -d '\r;')"
    ;;
  *) cp "$work/m0.json" "$work/m.json" ;;
esac
[ -n "$kind" ] || fail "type du manifeste" "Content-Type absent"
cfg_digest="$(jq -r '.config.digest' "$work/m.json")"
http "lire la config" "200" "$work/cfg.json" -L "$REG/v2/$REPO/blobs/$cfg_digest"

# 3. La couche : 6 Mo aléatoires (incompressibles), tar puis gzip ; empreintes du tar (diff_id) et du gzip.
mkdir "$work/layer"
head -c 6000000 /dev/urandom > "$work/layer/hearth-layer"
tar -C "$work/layer" -cf "$work/layer.tar" hearth-layer || fail "tar" "échec"
diff_id="sha256:$(sha256sum "$work/layer.tar" | cut -d' ' -f1)"
gzip -n -c "$work/layer.tar" > "$work/layer.tgz" || fail "gzip" "échec"
layer_digest="sha256:$(sha256sum "$work/layer.tgz" | cut -d' ' -f1)"
layer_size="$(stat -c %s "$work/layer.tgz")"
case "$kind" in
  *docker*) layer_type="application/vnd.docker.image.rootfs.diff.tar.gzip" ;;
  *) layer_type="application/vnd.oci.image.layer.v1.tar+gzip" ;;
esac
push_blob "envoyer la couche" "$work/layer.tgz" "$layer_digest"

# 4. La config avec la couche en plus, puis le manifeste ; le tag 1.36 du registre local pointe dessus.
jq -c --arg d "$diff_id" '.rootfs.diff_ids += [$d] | .history = ((.history // []) + [{"created_by":"hearth ci: couche de test (HRT-168)"}])' "$work/cfg.json" > "$work/cfg2.json" || fail "config" "jq a échoué"
cfg2_digest="sha256:$(sha256sum "$work/cfg2.json" | cut -d' ' -f1)"
cfg2_size="$(stat -c %s "$work/cfg2.json")"
push_blob "envoyer la config" "$work/cfg2.json" "$cfg2_digest"
jq -c --arg cd "$cfg2_digest" --argjson cs "$cfg2_size" --arg ld "$layer_digest" --argjson ls "$layer_size" --arg lt "$layer_type" \
  '.config.digest = $cd | .config.size = $cs | .layers += [{"mediaType":$lt,"size":$ls,"digest":$ld}]' "$work/m.json" > "$work/m2.json" || fail "manifeste" "jq a échoué"
http "envoyer le manifeste" "201" "$work/put.out" -X PUT -H "Content-Type: $kind" --data-binary "@$work/m2.json" "$REG/v2/$REPO/manifests/$TAGNAME"
# Le diff_id de la couche propre : le contrat vérifie qu'elle est dans l'image récupérée par le moteur.
if [ -n "${GITHUB_ENV:-}" ]; then echo "HEARTH_TEST_PULL_LAYER=$diff_id" >> "$GITHUB_ENV"; fi
echo "registre local prêt : localhost:5000/$REPO:$TAGNAME (couche propre de 6 Mo posée dans le registre, jamais vue du démon)"
