#!/bin/sh
# Scénario de bout en bout de la mise à jour de l'agent à distance (HRT-17), exécuté DANS le
# conteneur jetable de `cargo xtask e2e-update` (Debian + systemd réellement démarré). Monté en
# lecture seule :
#   /dist/hearth-agent-old    l'agent installé (0.1.0), qui n'accepte que la clé jetable du test
#   /dist/hearth-agent-new    la version suivante (0.2.0), vrai agent statique
#   /dist/new.minisig         sa signature (clé jetable) ; new.other.minisig : signée par une autre clé
#   /dist/mute                un « agent » signé qui annonce 0.3.0 mais ne répond jamais
#   /dist/mute.minisig        sa signature
#   /deploy/install.sh        le script d'installation
# Chaque vérification affiche « ok - … » ; la première qui échoue arrête tout (code 1).
set -eu

OLD=/dist/hearth-agent-old
PW='Cheval-Agrafe-42'
USER_NAME=marie
RO_NAME=lucas
PORT=7341
BASE="https://127.0.0.1:$PORT/api/v1"
INSTALLED=/usr/local/bin/hearth-agent
BACKUP=/usr/local/bin/.hearth-agent.previous
DATA=/var/lib/hearth
OUT=/tmp/out

ok() { printf 'ok - %s\n' "$*"; }
die() {
    printf 'ECHEC - %s\n' "$*" >&2
    [ -f "$OUT" ] && { echo '--- dernière sortie' >&2; cat "$OUT" >&2; }
    journalctl -u hearth-agent -u hearth-agent-update --no-pager 2>/dev/null | tail -n 40 >&2 || true
    exit 1
}

run() {
    status=0
    "$@" >"$OUT" 2>&1 || status=$?
    return "$status"
}

hello_version() {
    curl -sk --max-time 5 "$BASE/hello" | python3 -c 'import json,sys; print(json.load(sys.stdin)["agent_version"])'
}

json_field() {
    python3 -c 'import json,sys; d=json.load(sys.stdin)
for part in sys.argv[1].split("."):
    d = d[part] if isinstance(d, dict) else None
print("null" if d is None else (str(d).lower() if isinstance(d, bool) else d))' "$1"
}

token_of() {
    curl -sk --max-time 10 -X POST "$BASE/sessions" \
        -H 'content-type: application/json' -H 'x-hearth-api: 1' -H 'x-hearth-client: e2e/1' \
        -d "{\"username\":\"$1\",\"password\":\"$PW\"}" | json_field token
}

# Appel authentifié ; le corps de la réponse va dans $OUT.body, le code HTTP sur la sortie.
api() {
    method=$1; path=$2; token=$3; shift 3
    curl -sk --max-time 20 -o "$OUT.body" -w '%{http_code}' -X "$method" "$BASE$path" \
        -H "authorization: Bearer $token" -H 'x-hearth-api: 1' -H 'x-hearth-client: e2e/1' \
        -H 'content-type: application/json' "$@"
}

body_field() { json_field "$1" <"$OUT.body"; }

request_body() {
    # $1 version, $2 url, $3 fichier de signature, $4 fichier dont on donne la somme
    python3 -c 'import json,sys,hashlib
sig = open(sys.argv[3], encoding="utf-8").read()
data = open(sys.argv[4], "rb").read()
print(json.dumps({"version": sys.argv[1], "url": sys.argv[2], "signature": sig, "sha256": hashlib.sha256(data).hexdigest()}))' "$@"
}

sha_of() { sha256sum "$1" | cut -d ' ' -f 1; }

# L'agent EXIGE la confirmation des actes d'administration (HRT-30, ADR-0033) : mot de passe ET preuve de la
# clé d'un poste inscrit. `confirmed` ajoute le membre `reauth` à un corps de mise à jour (défi neuf, preuve
# liée à la version et à la somme, jeton de la session ADMIN). La clé de ce « poste » est inscrite une fois.
KEYFILE=/tmp/e2e-device.pem
confirmed() {
    printf '%s' "$1" | python3 /deploy/e2e/admin_act.py confirm 127.0.0.1 "$PORT" "$ADMIN" "$USER_NAME" "$PW" "$KEYFILE"
}

# Attend que la mise à jour en cours soit terminée (état « en cours » tombé, résultat présent).
wait_done() {
    waited=0
    while [ "$waited" -lt "$1" ]; do
        code=$(api GET /agent/update "$ADMIN" 2>/dev/null || true)
        if [ "$code" = 200 ] && [ "$(body_field in_progress)" = false ] && [ "$(body_field last)" != null ]; then
            return 0
        fi
        sleep 2
        waited=$((waited + 2))
    done
    die "la mise à jour ne s'est pas terminée en $1 s"
}

# Attend qu'un champ du résultat vaille cette valeur ET qu'aucune mise à jour ne soit en cours :
# $1 champ, $2 valeur, $3 délai maximal en secondes (un ancien résultat ne suffit pas).
wait_last() {
    waited=0
    while [ "$waited" -lt "$3" ]; do
        code=$(api GET /agent/update "$ADMIN" 2>/dev/null || true)
        if [ "$code" = 200 ] && [ "$(body_field in_progress)" = false ] && [ "$(body_field "$1")" = "$2" ]; then
            return 0
        fi
        sleep 1
        waited=$((waited + 1))
    done
    die "le résultat ($1 = $2) n'est pas arrivé en $3 s"
}

echo "== système : $(uname -m), systemd $(systemctl --version | head -n 1)"
[ -d /run/systemd/system ] || die "systemd n'est pas le système d'initialisation du conteneur"

# Serveur HTTPS local (certificat approuvé par le conteneur) qui sert les « versions ». Le chemin
# /slow/ envoie le fichier lentement : une mise à jour y reste « en cours » assez longtemps.
mkdir -p /tmp/pki /tmp/www
# Une autorité et un certificat de serveur distincts : le client de l'agent (webpki) refuse un
# certificat d'autorité utilisé comme certificat de serveur, comme il le ferait pour un vrai site.
openssl req -x509 -newkey rsa:2048 -nodes -keyout /tmp/pki/ca.key -out /tmp/pki/ca.pem -days 1 \
    -subj /CN=hearth-e2e-ca >/dev/null 2>&1
openssl req -newkey rsa:2048 -nodes -keyout /tmp/pki/key.pem -out /tmp/pki/server.csr -subj /CN=127.0.0.1 >/dev/null 2>&1
printf 'subjectAltName=IP:127.0.0.1
basicConstraints=CA:FALSE
extendedKeyUsage=serverAuth
' >/tmp/pki/server.ext
openssl x509 -req -in /tmp/pki/server.csr -CA /tmp/pki/ca.pem -CAkey /tmp/pki/ca.key -CAcreateserial \
    -days 1 -extfile /tmp/pki/server.ext -out /tmp/pki/cert.pem >/dev/null 2>&1
cp /tmp/pki/ca.pem /usr/local/share/ca-certificates/hearth-e2e.crt
update-ca-certificates >/dev/null 2>&1
cp /dist/hearth-agent-new /tmp/www/hearth-agent-new
cp /dist/mute /tmp/www/mute
cp /dist/hearth-agent-next /tmp/www/hearth-agent-next
python3 /deploy/e2e/https_server.py 8443 /tmp/www &
sleep 2
NEW_URL=https://127.0.0.1:8443/hearth-agent-new
SLOW_URL=https://127.0.0.1:8443/slow/hearth-agent-new
MUTE_URL=https://127.0.0.1:8443/mute
NEXT_URL=https://127.0.0.1:8443/hearth-agent-next

# --------------------------------------------------------------------------------------------
# Installation de l'agent d'avant (0.1.0), un compte administrateur et un compte en lecture seule
# --------------------------------------------------------------------------------------------
run env HEARTH_ADMIN_USER=$USER_NAME HEARTH_ADMIN_PASSWORD=$PW sh /deploy/install.sh \
    --binary "$OLD" --sha256 "$(sha_of "$OLD")" --yes || die "l'installation de l'agent d'avant a échoué"
grep -qF "Somme SHA-256 vérifiée (celle que tu as donnée)." "$OUT" || die "install.sh devrait dire que la somme est celle donnée"
[ "$(hello_version)" = "0.1.0" ] || die "l'agent installé n'est pas en 0.1.0"
run env HEARTH_ACCOUNT_PASSWORD=$PW "$INSTALLED" account add $RO_NAME --role readonly || die "création du compte en lecture seule"
ADMIN=$(token_of $USER_NAME)
READONLY=$(token_of $RO_NAME)
[ -n "$ADMIN" ] && [ -n "$READONLY" ] || die "connexion impossible"
[ "$(python3 /deploy/e2e/admin_act.py enroll 127.0.0.1 "$PORT" "$USER_NAME" "$PW" "$KEYFILE")" = enrolled ] || die "la clé de ce poste n'a pas été inscrite"
FP_BEFORE=$("$INSTALLED" fingerprint)
BIN_BEFORE=$(sha_of "$INSTALLED")
ACCOUNTS_BEFORE=$(api GET /accounts "$ADMIN" >/dev/null; python3 -c 'import json; print(len(json.load(open("/tmp/out.body"))["accounts"]))')
ok "agent 0.1.0 installé, deux comptes, empreinte $FP_BEFORE"

[ "$(api GET /agent/update "$READONLY")" = 200 ] || die "la lecture de l'état de mise à jour est refusée à un compte en lecture seule"
[ "$(body_field current)" = "0.1.0" ] && [ "$(body_field managed)" = false ] && [ "$(body_field in_progress)" = false ] \
    || die "état de départ inattendu : $(cat "$OUT.body")"
[ "$(api GET /agent/update/last "$READONLY")" = 200 ] && [ "$(body_field last)" = null ] || die "il ne devrait y avoir aucun résultat au départ"
ok "GET /agent/update et /agent/update/last : lisibles par un compte en lecture seule, rien en cours"

# --------------------------------------------------------------------------------------------
# 1. Un compte en lecture seule ne peut pas lancer ; une signature invalide est refusée AVANT toute écriture
# --------------------------------------------------------------------------------------------
BODY=$(request_body 0.2.0 "$NEW_URL" /dist/new.minisig /dist/hearth-agent-new)
code=$(api POST /agent/update "$READONLY" -d "$BODY")
[ "$code" = 403 ] && [ "$(body_field error.code)" = FORBIDDEN_ROLE ] || die "un compte en lecture seule devrait recevoir 403 FORBIDDEN_ROLE (reçu $code)"
ok "compte en lecture seule : 403 FORBIDDEN_ROLE"

# Un client qui n'envoie pas la confirmation (session seule) : l'agent EXIGE, « client trop ancien », rien n'est fait.
code=$(api POST /agent/update "$ADMIN" -d "$BODY")
[ "$code" = 426 ] && [ "$(body_field error.code)" = INCOMPATIBLE_VERSION ] && [ "$(body_field error.details.reason)" = reauth_required ]     || die "une demande sans confirmation devrait recevoir 426 INCOMPATIBLE_VERSION reauth_required (reçu $code : $(cat "$OUT.body"))"
[ ! -e "$DATA/update" ] || die "$DATA/update existe : quelque chose a été écrit pour une demande non confirmée"
ok "mise à jour sans confirmation : 426 client trop ancien, rien d'écrit"

: >/tmp/https.log
BAD=$(request_body 0.2.0 "$NEW_URL" /dist/new.other.minisig /dist/hearth-agent-new)
code=$(api POST /agent/update "$ADMIN" -d "$(confirmed "$BAD")")
[ "$code" = 422 ] && [ "$(body_field error.code)" = BAD_SIGNATURE ] || die "une signature d'une autre clé devrait recevoir 422 BAD_SIGNATURE (reçu $code : $(cat "$OUT.body"))"
[ ! -e "$DATA/update" ] || die "$DATA/update existe : quelque chose a été écrit avant la vérification"
[ ! -s /tmp/https.log ] || die "le fichier a été téléchargé malgré la signature invalide : $(cat /tmp/https.log)"
[ "$(sha_of "$INSTALLED")" = "$BIN_BEFORE" ] || die "le binaire installé a changé"
[ ! -e "$BACKUP" ] || die "une sauvegarde existe"
[ "$(hello_version)" = "0.1.0" ] || die "l'agent a changé de version"
ok "signature d'une autre clé : 422 BAD_SIGNATURE, rien téléchargé, rien écrit, binaire et version inchangés"

# Somme fausse (signature de la bonne clé, bon fichier) : refusée après téléchargement, avant toute écriture.
WRONG_SUM=$(python3 -c 'import json,sys; b=json.loads(sys.argv[1]); b["sha256"]="0"*64; print(json.dumps(b))' "$(request_body 0.2.0 "$NEW_URL" /dist/new.minisig /dist/hearth-agent-new)")
code=$(api POST /agent/update "$ADMIN" -d "$(confirmed "$WRONG_SUM")")
[ "$code" = 202 ] || die "la demande à somme fausse est acceptée puis échoue (reçu $code : $(cat "$OUT.body"))"
wait_done 60
[ "$(body_field last.outcome)" = failed ] && [ "$(body_field last.reason)" = bad_checksum ] || die "une somme fausse devrait donner failed/bad_checksum : $(cat "$OUT.body")"
[ ! -e "$DATA/update/hearth-agent.new" ] || die "un binaire à somme fausse a été déposé"
[ "$(sha_of "$INSTALLED")" = "$BIN_BEFORE" ] || die "le binaire installé a changé (somme fausse)"
ok "somme fausse : refusée avant toute écriture du binaire, agent inchangé"

# --------------------------------------------------------------------------------------------
# 2. Mise à jour réussie : comptes, journal et empreinte survivent
# --------------------------------------------------------------------------------------------
code=$(api POST /agent/update "$ADMIN" -d "$(confirmed "$BODY")")
[ "$code" = 202 ] || die "la mise à jour valable devrait être acceptée (202), reçu $code : $(cat "$OUT.body")"
[ "$(body_field step)" = download ] || die "la première étape devrait être le téléchargement"
wait_done 120
[ "$(body_field last.outcome)" = succeeded ] || die "la mise à jour devrait réussir : $(cat "$OUT.body")"
[ "$(body_field last.version)" = "0.2.0" ] && [ "$(body_field last.previous)" = "0.1.0" ] || die "versions du résultat inattendues"
[ "$(hello_version)" = "0.2.0" ] || die "le nouvel agent ne répond pas en 0.2.0"
[ "$(sha_of "$INSTALLED")" = "$(sha_of /dist/hearth-agent-new)" ] || die "le binaire installé n'est pas le nouveau"
[ ! -e "$BACKUP" ] || die "la sauvegarde de l'ancien binaire reste après une réussite"
[ "$(systemctl is-active hearth-agent)" = active ] || die "le service ne tourne pas"
[ "$("$INSTALLED" fingerprint)" = "$FP_BEFORE" ] || die "l'empreinte a changé"
ok "mise à jour 0.1.0 vers 0.2.0 : réussie, empreinte inchangée, ancienne sauvegarde retirée"

# Le jeton d'avant le redémarrage vaut toujours (les comptes et les sessions sont dans la base).
ACCOUNTS_AFTER=$(api GET /accounts "$ADMIN" >/dev/null; python3 -c 'import json; print(len(json.load(open("/tmp/out.body"))["accounts"]))')
[ "$ACCOUNTS_BEFORE" = "$ACCOUNTS_AFTER" ] || die "le nombre de comptes a changé ($ACCOUNTS_BEFORE puis $ACCOUNTS_AFTER)"
[ "$(token_of $USER_NAME)" != "" ] || die "le compte administrateur ne se connecte plus"
code=$(api GET "/audit?action=agent.update" "$ADMIN")
[ "$code" = 200 ] || die "lecture du journal impossible ($code)"
python3 - "$OUT.body" <<'PY' || die "le journal devrait garder les entrées de la mise à jour"
import json, sys
entries = json.load(open(sys.argv[1]))["events"]
outcomes = [e["outcome"] for e in entries if e["action"] == "agent.update"]
assert outcomes.count("ok") == 1, outcomes
assert outcomes.count("failed") >= 1, outcomes
assert any(e["account"] == "marie" for e in entries), entries
PY
ok "comptes, sessions et journal intacts ; le journal garde la mise à jour réussie (et le refus avant)"

# --------------------------------------------------------------------------------------------
# 3. Le nouvel agent ne répond pas : retour automatique à la version précédente
# --------------------------------------------------------------------------------------------
BIN_V2=$(sha_of "$INSTALLED")
MUTE_BODY=$(request_body 0.3.0 "$MUTE_URL" /dist/mute.minisig /dist/mute)
code=$(api POST /agent/update "$ADMIN" -d "$(confirmed "$MUTE_BODY")")
[ "$code" = 202 ] || die "la mise à jour vers l'agent muet devrait être acceptée (202), reçu $code : $(cat "$OUT.body")"
# On attend, sur condition (pas sur une durée), que le binaire installé soit celui de l'agent muet :
# l'ancien agent est alors arrêté et remplacé, puis /hello ne répond pas.
waited=0
until [ "$(sha_of "$INSTALLED")" = "$(sha_of /dist/mute)" ]; do
    [ "$waited" -lt 60 ] || die "le binaire de l'agent muet n'a jamais été installé"
    sleep 1
    waited=$((waited + 1))
done
if hello_version >/dev/null 2>&1; then
    die "l'agent muet ne devrait pas répondre"
fi
ok "pendant le contrôle : le nouvel agent ne répond pas, l'ancien binaire est remplacé"
wait_done 150
[ "$(body_field last.outcome)" = rolled_back ] && [ "$(body_field last.reason)" = no_answer ] || die "le retour arrière devrait être enregistré : $(cat "$OUT.body")"
[ "$(hello_version)" = "0.2.0" ] || die "l'agent d'avant ne répond plus après le retour en arrière"
[ "$(sha_of "$INSTALLED")" = "$BIN_V2" ] || die "le binaire restauré n'est pas exactement l'ancien"
[ ! -e "$BACKUP" ] || die "une sauvegarde reste après le retour en arrière"
[ "$(systemctl is-active hearth-agent)" = active ] || die "le service ne tourne pas après le retour en arrière"
[ "$("$INSTALLED" fingerprint)" = "$FP_BEFORE" ] || die "l'empreinte a changé"
[ "$(api GET /agent/update/last "$READONLY")" = 200 ] && [ "$(body_field last.outcome)" = rolled_back ] || die "le résultat n'est plus lisible"
code=$(api GET "/audit?action=agent.update&outcome=failed" "$ADMIN")
python3 - "$OUT.body" <<'PY' || die "le journal devrait garder le retour en arrière"
import json, sys
entries = json.load(open(sys.argv[1]))["events"]
assert any("retour à la version précédente" in (e.get("reason") or "") for e in entries), entries
PY
ok "agent muet : retour automatique à l'ancien binaire (identique octet pour octet), service actif, empreinte inchangée, retour consigné au journal"

# --------------------------------------------------------------------------------------------
# 4. Une seule mise à jour à la fois
# --------------------------------------------------------------------------------------------
SLOW_BODY=$(request_body 0.9.0 "$SLOW_URL" /dist/new.minisig /dist/hearth-agent-new)
code=$(api POST /agent/update "$ADMIN" -d "$(confirmed "$SLOW_BODY")")
[ "$code" = 202 ] || die "la mise à jour lente devrait être acceptée (reçu $code : $(cat "$OUT.body"))"
sleep 1
code=$(api POST /agent/update "$ADMIN" -d "$(confirmed "$SLOW_BODY")")
[ "$code" = 409 ] && [ "$(body_field error.code)" = OPERATION_IN_PROGRESS ] || die "la deuxième demande devrait recevoir 409 OPERATION_IN_PROGRESS (reçu $code)"
[ "$(body_field error.message)" = "Une mise à jour de l'agent est déjà en cours. Réessaye plus tard." ] || die "message inattendu : $(cat "$OUT.body")"
[ "$(api GET /agent/update "$READONLY")" = 200 ] && [ "$(body_field in_progress)" = true ] || die "l'état devrait dire « en cours »"
wait_done 120
# Le binaire annonce 0.2.0, pas 0.9.0 : refusé avant tout échange.
[ "$(body_field last.outcome)" = failed ] && [ "$(body_field last.reason)" = bad_binary ] || die "un binaire qui n'annonce pas la version visée devrait être refusé : $(cat "$OUT.body")"
[ "$(sha_of "$INSTALLED")" = "$BIN_V2" ] || die "le binaire installé a changé"
code=$(api GET "/audit?action=agent.update" "$ADMIN")
python3 - "$OUT.body" <<'PY' || die "le refus « déjà en cours » devrait être au journal"
import json, sys
entries = json.load(open(sys.argv[1]))["events"]
assert any("déjà en cours" in (e.get("reason") or "") for e in entries), entries
PY
ok "deuxième demande pendant une mise à jour : 409 OPERATION_IN_PROGRESS, consignée ; la première finit seule"

# --------------------------------------------------------------------------------------------
# 5. Superviseur tué après l'échange des binaires : le démarrage suivant conclut (BR-UPDATE-028)
# --------------------------------------------------------------------------------------------
NEXT_BODY=$(request_body 0.2.1 "$NEXT_URL" /dist/next.minisig /dist/hearth-agent-next)
code=$(api POST /agent/update "$ADMIN" -d "$(confirmed "$NEXT_BODY")")
[ "$code" = 202 ] || die "la mise à jour vers 0.2.1 devrait être acceptée (reçu $code : $(cat "$OUT.body"))"
# Dès que le binaire est échangé, le superviseur est gelé puis tué : il ne conclura jamais.
waited=0
until [ "$(sha_of "$INSTALLED")" = "$(sha_of /dist/hearth-agent-next)" ]; do
    [ "$waited" -lt 600 ] || die "le binaire 0.2.1 n'a jamais été installé"
    sleep 0.1
    waited=$((waited + 1))
done
systemctl kill --signal=SIGSTOP hearth-agent-update 2>/dev/null || true
# Un arrêt PROPRE de l'unité : systemd ne la relance pas (un SIGKILL la relancerait, `Restart=on-failure`,
# HRT-27 : c'est la section 7). Ici, le superviseur est mort et la machine « redémarre » : personne ne le relance.
systemctl stop hearth-agent-update 2>/dev/null || true
systemctl reset-failed hearth-agent-update 2>/dev/null || true
[ -e "$BACKUP" ] || die "le superviseur a conclu avant d'être tué : la sauvegarde de l'ancien binaire a disparu"
ok "superviseur tué juste après l'échange : sauvegarde de l'ancien binaire et travail laissés"
# Le serveur redémarre (service arrêté puis relancé) : le nouvel agent doit conclure seul.
systemctl stop hearth-agent
systemctl start hearth-agent
waited=0
until [ "$(hello_version 2>/dev/null)" = "0.2.1" ]; do
    [ "$waited" -lt 60 ] || die "le nouvel agent 0.2.1 ne répond pas après le redémarrage"
    sleep 1
    waited=$((waited + 1))
done
wait_last last.version 0.2.1 120
[ "$(body_field last.outcome)" = succeeded ] && [ "$(body_field last.version)" = "0.2.1" ] || die "la mise à jour orpheline devrait être conclue en réussite : $(cat "$OUT.body")"
[ ! -e "$BACKUP" ] || die "la sauvegarde de l'ancien binaire reste après la conclusion"
[ ! -e "$DATA/update/job.json" ] && [ ! -e "$DATA/update/state.json" ] || die "des traces de la mise à jour restent dans $DATA/update"
[ "$("$INSTALLED" fingerprint)" = "$FP_BEFORE" ] || die "l'empreinte a changé"
code=$(api GET "/audit?action=agent.update&outcome=ok" "$ADMIN")
python3 - "$OUT.body" <<'PY' || die "la mise à jour conclue au démarrage devrait être au journal, au nom de marie"
import json, sys
entries = json.load(open(sys.argv[1]))["events"]
assert any(e["account"] == "marie" and e["target"] == "version 0.2.1" for e in entries), entries
PY
ok "mise à jour orpheline : conclue au démarrage (réussite), sauvegarde retirée, journal au nom du demandeur"

# --------------------------------------------------------------------------------------------
# 6. Agent tué pendant le téléchargement : la tentative est conclue « interrompue » au démarrage
# --------------------------------------------------------------------------------------------
code=$(api POST /agent/update "$ADMIN" -d "$(confirmed "$SLOW_BODY")")
[ "$code" = 202 ] || die "la mise à jour lente devrait être acceptée (reçu $code : $(cat "$OUT.body"))"
sleep 2
systemctl kill --signal=SIGKILL hearth-agent
waited=0
until [ "$(hello_version 2>/dev/null)" = "0.2.1" ]; do
    [ "$waited" -lt 90 ] || die "l'agent ne revient pas après avoir été tué"
    sleep 1
    waited=$((waited + 1))
done
wait_last last.reason interrupted 60
[ "$(body_field last.outcome)" = failed ] && [ "$(body_field last.reason)" = interrupted ] || die "la tentative interrompue devrait être conclue : $(cat "$OUT.body")"
[ ! -e "$DATA/update/state.json" ] || die "la trace d'intention reste"
[ "$(sha_of "$INSTALLED")" = "$(sha_of /dist/hearth-agent-next)" ] || die "le binaire installé a changé"
ok "agent tué pendant le téléchargement : tentative conclue « interrompue » au démarrage, rien d'écrit sur le binaire"

# --------------------------------------------------------------------------------------------
# 7. Double panne (HRT-27) : le superviseur est TUÉ après l'échange et le nouvel agent ne répond pas.
#    systemd relance l'unité transitoire (`Restart=on-failure`), le superviseur reprend son marqueur et
#    remet l'ancien binaire : le service est relevé sans geste manuel.
# --------------------------------------------------------------------------------------------
BIN_V021=$(sha_of "$INSTALLED")
code=$(api POST /agent/update "$ADMIN" -d "$(confirmed "$MUTE_BODY")")
[ "$code" = 202 ] || die "la mise à jour vers l'agent muet devrait être acceptée (202), reçu $code : $(cat "$OUT.body")"
waited=0
until [ "$(sha_of "$INSTALLED")" = "$(sha_of /dist/mute)" ]; do
    [ "$waited" -lt 600 ] || die "le binaire de l'agent muet n'a jamais été installé"
    sleep 0.1
    waited=$((waited + 1))
done
systemctl kill --signal=SIGKILL hearth-agent-update 2>/dev/null || die "le superviseur n'existe plus : il a déjà conclu"
ok "superviseur tué (SIGKILL) après l'échange : le nouvel agent ne répond pas, personne ne conclut... sauf systemd"
# Aucun geste manuel à partir d'ici : on ne touche ni au service ni aux fichiers.
waited=0
until [ "$(hello_version 2>/dev/null)" = "0.2.1" ]; do
    [ "$waited" -lt 180 ] || die "le service n'a pas été relevé seul en 180 s"
    sleep 1
    waited=$((waited + 1))
done
wait_last last.outcome rolled_back 60
[ "$(body_field last.reason)" = no_answer ] || die "raison inattendue : $(cat "$OUT.body")"
[ "$(sha_of "$INSTALLED")" = "$BIN_V021" ] || die "le binaire restauré n'est pas exactement l'ancien"
[ ! -e "$BACKUP" ] || die "une sauvegarde reste après le retour arrière"
[ "$(systemctl is-active hearth-agent)" = active ] || die "le service ne tourne pas"
journalctl -u hearth-agent-update --no-pager | grep -qF "reprise du superviseur" || die "le superviseur n'a pas été repris par systemd"
systemctl is-active --quiet hearth-agent-update && die "l'unité du superviseur tourne encore"
[ ! -e "$DATA/update/phase.json" ] && [ ! -e "$DATA/update/job.json" ] || die "des traces de travail restent dans $DATA/update"
[ "$("$INSTALLED" fingerprint)" = "$FP_BEFORE" ] || die "l'empreinte a changé"
ok "double panne : superviseur relancé par systemd, ancien binaire remis (identique octet pour octet), service relevé sans geste manuel"

echo "== mise à jour de l'agent à distance : vert"
