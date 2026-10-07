#!/bin/sh
# La double panne de la mise à jour de l'agent (HRT-27, BR-UPDATE-030 à 034) sur un VRAI systemd :
# le superviseur est tué pendant le contrôle d'un nouveau binaire qui ne démarre pas ; systemd le
# relance (`Restart=on-failure` sur son unité transitoire), il reprend son marqueur et remet l'ancien
# binaire, sans geste manuel.
#
# Exécuté en root sur une machine Linux avec systemd (une machine jetable : conteneur Debian de
# `cargo xtask e2e-update`, ou une distribution WSL2 avec `systemd=true`). Il INSTALLE l'agent, puis le
# DÉSINSTALLE avec `--purge` à la fin (même en cas d'échec).
#   usage : double-failure-systemd.sh /chemin/vers/hearth-agent
#
# Le travail est écrit à la main (pas d'API, donc ni clé de signature ni serveur de versions) et le
# superviseur est lancé par `systemd-run` avec les propriétés de `systemd_run_arguments`
# (crates/hearth-agent/src/infrastructure/update/host.rs) : GARDER LES DEUX EN PHASE. Le scénario
# complet par l'API, avec le vrai lancement, est la section 7 de `scenario-update.sh`.
set -eu

AGENT_SRC=${1:?usage: double-failure-systemd.sh /chemin/vers/hearth-agent}
PW='Cheval-Agrafe-42'
PORT=7341
BASE="https://127.0.0.1:$PORT/api/v1"
INSTALLED=/usr/local/bin/hearth-agent
BACKUP=/usr/local/bin/.hearth-agent.previous
DATA=/var/lib/hearth
UPD=$DATA/update
UNIT_SUP=hearth-agent-update
WORK=$(mktemp -d)
# Le journal de systemd garde les passages précédents : on ne compte que ce que CE passage a écrit.
START=$(date '+%Y-%m-%d %H:%M:%S')

ok() { printf 'ok - %s\n' "$*"; }
die() {
    printf 'ECHEC - %s\n' "$*" >&2
    journalctl -u hearth-agent -u "$UNIT_SUP" --no-pager 2>/dev/null | tail -n 40 >&2 || true
    exit 1
}

cleanup() {
    status=$?
    systemctl stop "$UNIT_SUP" >/dev/null 2>&1 || true
    systemctl reset-failed "$UNIT_SUP" >/dev/null 2>&1 || true
    if [ -x "$INSTALLED" ]; then
        "$AGENT_SRC" uninstall --purge --yes >/dev/null 2>&1 || true
    fi
    rm -rf "$UPD" "$BACKUP" "$WORK"
    exit "$status"
}
trap cleanup EXIT

[ "$(id -u)" = 0 ] || die "à lancer en root"
[ -d /run/systemd/system ] || die "systemd n'est pas le système d'initialisation"
echo "== $(uname -r), $(systemctl --version | head -n 1)"

hello_version() {
    curl -sk --max-time 3 "$BASE/hello" | python3 -c 'import json,sys; print(json.load(sys.stdin)["agent_version"])'
}
sha_of() { sha256sum "$1" | cut -d ' ' -f 1; }
wait_for() { # $1 description, $2 délai en s, le reste : commande qui doit réussir
    desc=$1; limit=$2; shift 2
    waited=0
    until "$@" >/dev/null 2>&1; do
        [ "$waited" -lt "$limit" ] || die "$desc : pas atteint en $limit s"
        sleep 1
        waited=$((waited + 1))
    done
}
unit_gone() { ! systemctl is-active --quiet "$UNIT_SUP" && ! systemctl is-failed --quiet "$UNIT_SUP"; }
phase_is() { grep -q "\"phase\": \"$1\"" "$UPD/phase.json" 2>/dev/null; }
phase_field() { python3 -c 'import json; print(json.load(open("'"$UPD"'/phase.json"))["'"$1"'"])'; }
journal_count() { journalctl -u "$UNIT_SUP" --since "$START" --no-pager 2>/dev/null | grep -c -F -- "$1" || true; }

# --------------------------------------------------------------------------------------------
# Installation de l'agent d'avant
# --------------------------------------------------------------------------------------------
HEARTH_ADMIN_USER=marie HEARTH_ADMIN_PASSWORD=$PW "$AGENT_SRC" install --yes >"$WORK/install.out" 2>&1 \
    || { cat "$WORK/install.out" >&2; die "l'installation de l'agent a échoué"; }
wait_for "l'agent installé répond" 30 hello_version
OLD_VERSION=$(hello_version)
OLD_SHA=$(sha_of "$INSTALLED")
FP=$(openssl x509 -in "$DATA/cert.pem" -outform DER | sha256sum | cut -d ' ' -f 1)
ok "agent $OLD_VERSION installé et actif, empreinte $FP"

# Un « nouveau binaire » : `crash` ne démarre pas (le cas de la double panne), `mute` démarre sans jamais
# répondre. Le service a `Restart=always` : un binaire qui plante est relancé toutes les 5 s.
printf '#!/bin/sh\nexit 1\n' >"$WORK/crash"
printf '#!/bin/sh\nexec sleep 100000\n' >"$WORK/mute"
chmod 0755 "$WORK/crash" "$WORK/mute"

# Prépare une mise à jour vers `$1` (le binaire déposé) : copie du superviseur, travail, puis lance l'unité
# transitoire avec ses propriétés de production. `$2` : fenêtre de contrôle en ms.
start_update() {
    START=$(date '+%Y-%m-%d %H:%M:%S')
    mkdir -p "$UPD"
    chmod 0700 "$UPD"
    cp "$1" "$UPD/hearth-agent.new"
    chmod 0700 "$UPD/hearth-agent.new"
    cp "$INSTALLED" "$UPD/supervisor"
    chmod 0700 "$UPD/supervisor"
    cat >"$UPD/job.json" <<JSON
{
  "version": "${4:-9.9.9}",
  "previous": "${5:-$OLD_VERSION}",
  "binary": "$INSTALLED",
  "staged": "$UPD/hearth-agent.new",
  "backup": "$BACKUP",
  "probe_addr": "${3:-127.0.0.1:$PORT}",
  "fingerprint": "$FP",
  "grace_ms": 0,
  "check_window_ms": $2,
  "poll_ms": 500,
  "requested_by": "e2e"
}
JSON
    systemd-run --unit "$UNIT_SUP" --description "Mise à jour de l'agent Hearth" --collect --quiet \
        --service-type=exec --property=Restart=on-failure --property=RestartSec=2 \
        --property=StartLimitIntervalSec=600 --property=StartLimitBurst=6 \
        --property=NoNewPrivileges=yes --property=PrivateTmp=yes --property=ProtectHome=yes \
        "$UPD/supervisor" update-supervise --job "$UPD/job.json"
}

# Remet le serveur à l'état d'avant, pour le scénario suivant.
reset_server() {
    systemctl stop "$UNIT_SUP" >/dev/null 2>&1 || true
    systemctl reset-failed "$UNIT_SUP" >/dev/null 2>&1 || true
    systemctl stop hearth-agent >/dev/null 2>&1 || true
    cp -p "$WORK/installed.orig" "$INSTALLED"
    rm -rf "$UPD" "$BACKUP"
    systemctl reset-failed hearth-agent >/dev/null 2>&1 || true
    systemctl start hearth-agent
    wait_for "l'agent d'avant répond de nouveau" 30 hello_version
}
cp -p "$INSTALLED" "$WORK/installed.orig"
# ONLY=<n> : ne joue que la section n (1, 2, 3 ou 4).
want() { [ -z "${ONLY:-}" ] || [ "$ONLY" = "$1" ]; }

if want 1; then
# --------------------------------------------------------------------------------------------
# 1. LA DOUBLE PANNE : le superviseur est tué pendant le contrôle d'un binaire qui ne démarre pas
# --------------------------------------------------------------------------------------------
start_update "$WORK/crash" 12000
systemctl show "$UNIT_SUP" -p Restart -p StartLimitBurst | tr '\n' ' ' | grep -q 'Restart=on-failure' \
    || die "l'unité transitoire n'a pas Restart=on-failure : $(systemctl show "$UNIT_SUP" -p Restart)"
ok "unité transitoire acceptée par systemd : $(systemctl show "$UNIT_SUP" -p Restart -p RestartUSec -p StartLimitBurst | tr '\n' ' ')"
wait_for "le binaire déposé est installé et le contrôle commence" 30 phase_is checking
[ "$(sha_of "$INSTALLED")" = "$(sha_of "$WORK/crash")" ] || die "l'échange n'a pas eu lieu"
hello_version >/dev/null 2>&1 && die "le binaire qui plante ne devrait pas répondre"
SUP_PID=$(systemctl show "$UNIT_SUP" -p MainPID --value)
systemctl kill --signal=SIGKILL "$UNIT_SUP"
ok "superviseur tué (SIGKILL, pid $SUP_PID) après l'échange, pendant le contrôle : le nouveau binaire ne démarre pas"
wait_for "systemd relance le superviseur (reprise)" 20 sh -c "[ \"\$(journalctl -u $UNIT_SUP --since \"$START\" --no-pager | grep -c 'reprise du superviseur')\" -ge 1 ]"
ok "systemd a relancé le superviseur, qui a repris son marqueur"
# La fenêtre de contrôle (12 s) repart, le retour arrière suit : sans aucun geste manuel.
wait_for "le service est relevé (retour arrière)" 90 sh -c "[ \"\$(curl -sk --max-time 3 $BASE/hello | python3 -c 'import json,sys; print(json.load(sys.stdin)[\"agent_version\"])')\" = '$OLD_VERSION' ]"
wait_for "l'unité du superviseur a disparu" 20 unit_gone
[ "$(sha_of "$INSTALLED")" = "$OLD_SHA" ] || die "le binaire restauré n'est pas exactement l'ancien"
[ ! -e "$BACKUP" ] || die "la sauvegarde de l'ancien binaire reste"
[ "$(systemctl is-active hearth-agent)" = active ] || die "le service n'est pas actif"
grep -q '"outcome": "rolled_back"' "$UPD/last.json" || die "le résultat n'est pas rolled_back : $(cat "$UPD/last.json")"
[ ! -e "$UPD/phase.json" ] && [ ! -e "$UPD/job.json" ] || die "des traces de travail restent : $(ls "$UPD")"
ok "DOUBLE PANNE : service relevé sans geste manuel, ancien binaire identique octet pour octet, résultat rolled_back, unité du superviseur collectée"
reset_server
fi

if want 2; then
# --------------------------------------------------------------------------------------------
# 2. Un arrêt voulu du service n'est jamais défait
# --------------------------------------------------------------------------------------------
start_update "$WORK/mute" 40000
wait_for "le contrôle commence" 30 phase_is checking
systemctl stop hearth-agent
wait_for "le superviseur se met en pause (sortie en 0, unité collectée)" 30 unit_gone
sleep 6
[ "$(systemctl is-active hearth-agent || true)" = inactive ] || die "le service a été relancé : $(systemctl is-active hearth-agent)"
[ "$(sha_of "$INSTALLED")" = "$(sha_of "$WORK/mute")" ] || die "un retour arrière a eu lieu malgré l'arrêt voulu"
[ ! -e "$UPD/last.json" ] || die "un résultat a été écrit : $(cat "$UPD/last.json")"
[ -e "$BACKUP" ] && [ -e "$UPD/job.json" ] || die "le travail ou l'ancien binaire ont disparu : la reprise au démarrage de l'agent est perdue"
[ "$(journal_count 'service arrêté à la main')" -ge 1 ] || die "l'arrêt voulu n'est pas journalisé"
ok "systemctl stop hearth-agent pendant le contrôle : rien n'est défait, le service reste arrêté, ancien binaire et travail gardés pour la reprise au démarrage"
reset_server
fi

if want 2; then
# 2 bis. Arrêter l'unité du superviseur elle-même n'est pas une panne : systemd ne la relance pas.
start_update "$WORK/mute" 40000
wait_for "le contrôle commence" 30 phase_is checking
systemctl stop "$UNIT_SUP"
sleep 6
unit_gone || die "l'unité du superviseur a été relancée après un systemctl stop"
ok "systemctl stop hearth-agent-update : arrêt propre, pas de relance"
reset_server
fi

if want 3; then
# --------------------------------------------------------------------------------------------
# 3. Pas de boucle : reprises bornées, puis « reprise abandonnée, copies gardées » journalisé une fois
# --------------------------------------------------------------------------------------------
start_update "$WORK/mute" 60000
wait_for "le contrôle commence" 30 phase_is checking
kills=0
while [ "$kills" -lt 12 ]; do
    if systemctl is-active --quiet "$UNIT_SUP"; then
        systemctl kill --signal=SIGKILL "$UNIT_SUP" >/dev/null 2>&1 || true
        kills=$((kills + 1))
        sleep 3
    else
        [ "$(systemctl is-active "$UNIT_SUP" || true)" = activating ] && { sleep 1; continue; }
        break
    fi
done
wait_for "le superviseur abandonne" 40 unit_gone
[ "$(journal_count 'reprise de la mise à jour abandonnée')" = 1 ] || die "l'abandon devrait être journalisé UNE fois : $(journal_count 'reprise de la mise à jour abandonnée')"
[ "$(phase_field phase)" = abandoned ] || die "le marqueur devrait dire abandoned"
grep -q '"reason": "rollback_failed"' "$UPD/last.json" || die "résultat attendu : failed / rollback_failed : $(cat "$UPD/last.json")"
[ -e "$BACKUP" ] && [ -e "$UPD/hearth.db.before" ] || die "les copies (ancien binaire, base d'avant) devraient être gardées"
sleep 8
unit_gone || die "quelque chose relance encore le superviseur"
[ "$(journal_count 'reprise de la mise à jour abandonnée')" = 1 ] || die "l'abandon a été journalisé plusieurs fois"
ok "$kills mises à mort : reprises bornées, abandon journalisé une seule fois, copies gardées, plus rien ne relance"
reset_server
fi

if want 4; then
# --------------------------------------------------------------------------------------------
# 4. L'AGENT QUI DÉMARRE PENDANT QUE SYSTEMD S'APPRÊTE À RELANCER LE SUPERVISEUR (pré-review HRT-27)
#    Le nouvel agent est un VRAI agent (version 0.x.y+1) : il atteint `resume()` à chaque démarrage. Le
#    contrôle ne peut pas aboutir (adresse de contrôle sans écoute). Le superviseur est tué deux fois ;
#    à chaque fois le service est redémarré dans les 2 s avant la relance de l'unité : l'agent ne doit
#    ni réécrire le travail ni la copie du superviseur, et le retour arrière doit avoir lieu.
# --------------------------------------------------------------------------------------------
NEW_VERSION=$(echo "$OLD_VERSION" | awk -F. '{printf "%s.%s.%s", $1, $2, $3 + 1}')
[ "${#NEW_VERSION}" = "${#OLD_VERSION}" ] || die "les versions n'ont pas la même longueur : $OLD_VERSION, $NEW_VERSION"
sed "s/$OLD_VERSION/$NEW_VERSION/g" "$INSTALLED" >"$WORK/agent-new"
chmod 0755 "$WORK/agent-new"
"$WORK/agent-new" --version | grep -qF "$NEW_VERSION" || die "le binaire modifié n'annonce pas $NEW_VERSION : $("$WORK/agent-new" --version)"
start_update "$WORK/agent-new" 20000 127.0.0.1:7999 "$NEW_VERSION" "$OLD_VERSION"
wait_for "le contrôle commence" 30 phase_is checking
n=0
while [ "$n" -lt 2 ]; do
    n=$((n + 1))
    systemctl kill --signal=SIGKILL "$UNIT_SUP"
    systemctl restart hearth-agent
    wait_for "reprise $n par systemd" 30 sh -c "[ \"\$(journalctl -u $UNIT_SUP --since \"$START\" --no-pager | grep -c 'reprise du superviseur')\" -ge $n ]"
    ok "mort $n du superviseur, agent redémarré dans la fenêtre de relance, reprise $n par systemd"
done
wait_for "le superviseur conclut" 150 unit_gone
[ "$(sha_of "$INSTALLED")" = "$OLD_SHA" ] || die "le retour arrière n'a pas eu lieu : le binaire en place n'est pas l'ancien"
wait_for "l'ancien agent répond" 60 sh -c "[ \"\$(curl -sk --max-time 3 $BASE/hello | python3 -c 'import json,sys; print(json.load(sys.stdin)[\"agent_version\"])')\" = '$OLD_VERSION' ]"
[ "$(systemctl is-active hearth-agent)" = active ] || die "le service n'est pas actif"
[ ! -e "$UPD/job.json" ] || die "le travail reste"
ok "agent redémarré pendant l'attente de relance, deux fois : ni travail ni superviseur réécrits, retour arrière fait par le superviseur ($(python3 -c 'import json; d=json.load(open("'"$UPD"'/last.json")); print(d["outcome"], d.get("reason"))'))"
reset_server
fi

echo "== double panne de la mise à jour de l'agent : vert"
