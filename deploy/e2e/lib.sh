# Fonctions communes des scénarios de bout en bout (HRT-206), SOURCÉES par scenario-update.sh et
# double-failure-systemd.sh (sh POSIX, pas de bashisme). Le script appelant pose `set -eu`.
# shellcheck shell=sh

# SIGKILL au processus principal d'une unité, par son pid. `systemctl kill` parcourt AUSSI le cgroup et sort en
# erreur (« Failed to send signal SIGKILL to auxiliary processes: Input/output error ») quand un fil disparaît
# pendant le parcours, même si le processus principal a reçu le signal. Un MainPID vide ou égal à 0 (unité entre
# deux relances) est refusé : `kill -KILL 0` tuerait tout le groupe de processus, le script compris.
# $1 unité ; réessaie jusqu'à 2 s tant qu'il n'y a pas de pid ; code 1 si aucun pid trouvé.
kill_main() {
    tries=0
    while [ "$tries" -lt 10 ]; do
        pid=$(systemctl show -p MainPID --value "$1" 2>/dev/null || true)
        case "$pid" in
            '' | 0 | *[!0-9]*) ;;
            *)
                if kill -KILL "$pid" 2>/dev/null; then
                    echo "kill -KILL $pid ($1)"
                    return 0
                fi
                ;;
        esac
        sleep 0.2
        tries=$((tries + 1))
    done
    return 1
}

# À appeler en tête du trap EXIT avec le code de sortie. Une sortie en erreur hors de `die` (commande en échec
# sous `set -e`) ne doit pas être muette : code, dernière sortie ($OUT) et fin du journal des unités.
# Variables lues : DIED (1 si `die` a déjà tout affiché), OUT (fichier de sortie, facultatif), LIB_UNITS (unités).
report_exit() {
    [ "$1" -ne 0 ] || return 0
    [ "${DIED:-0}" -ne 1 ] || return 0
    echo "ECHEC - sortie inattendue (code $1), sans passer par die" >&2
    if [ -n "${OUT:-}" ] && [ -f "$OUT" ]; then
        echo '--- dernière sortie' >&2
        cat "$OUT" >&2
    fi
    set --
    for unit in ${LIB_UNITS:-hearth-agent hearth-agent-update}; do
        set -- "$@" -u "$unit"
    done
    echo '--- journal' >&2
    journalctl "$@" --no-pager 2>/dev/null | tail -n 40 >&2 || true
}
