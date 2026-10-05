# Machine jetable du test de bout en bout de l'installation (`cargo xtask e2e-install`) :
# un Debian avec systemd réellement démarré, comme un vrai serveur. Aucun binaire de l'agent
# n'est dans l'image : il est monté depuis `target/dist/`.
FROM debian:12-slim

ENV container=docker
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        systemd systemd-sysv dbus curl ca-certificates openssl netcat-openbsd python3 procps util-linux \
    && rm -rf /var/lib/apt/lists/* \
    # Les unités inutiles dans un conteneur ralentissent ou bloquent le démarrage.
    && rm -f /lib/systemd/system/multi-user.target.wants/* \
        /etc/systemd/system/*.wants/* \
        /lib/systemd/system/local-fs.target.wants/* \
        /lib/systemd/system/sockets.target.wants/*udev* \
        /lib/systemd/system/sockets.target.wants/*initctl* \
        /lib/systemd/system/sysinit.target.wants/systemd-tmpfiles-setup.service \
        /lib/systemd/system/systemd-update-utmp*

STOPSIGNAL SIGRTMIN+3
CMD ["/sbin/init"]
