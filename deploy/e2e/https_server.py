"""Serveur HTTPS minimal des scénarios de bout en bout : sert un dossier, journalise les chemins
demandés dans /tmp/https.log, redirige /redirect vers une adresse en clair (le script
d'installation doit le refuser) et sert /slow/<fichier> lentement (une mise à jour y reste « en
cours » assez longtemps pour qu'une deuxième demande arrive). Trois préfixes servent la lecture
du pointeur sans curl (HRT-59) : /chunked/<chemin> sert le fichier en morceaux, SANS longueur
annoncée ; /redir/<chemin> redirige vers la même adresse en HTTPS ; /redirhttp/<chemin> redirige
vers la même adresse en clair (le script d'installation doit le refuser). Certificat : /tmp/pki,
approuvé par le conteneur."""

import http.server
import os
import ssl
import sys
import time

port = int(sys.argv[1])
root = sys.argv[2]


class Handler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/redirect":
            self.send_response(302)
            self.send_header("Location", "http://127.0.0.1:1/hearth-agent")
            self.end_headers()
            return
        if self.path.startswith("/chunked/"):
            self.serve_chunked(self.path[len("/chunked/"):])
            return
        if self.path.startswith("/redir/") or self.path.startswith("/redirhttp/"):
            scheme = "https" if self.path.startswith("/redir/") else "http"
            target = self.path[self.path.index("/", 1):]
            self.send_response(302)
            self.send_header("Location", "%s://127.0.0.1:%d%s" % (scheme, port, target))
            self.end_headers()
            return
        if self.path.startswith("/slow/"):
            self.serve_slowly(self.path[len("/slow/"):])
            return
        super().do_GET()

    def serve_chunked(self, name):
        path = os.path.join(root, name)
        if not os.path.isfile(path):
            self.send_error(404)
            return
        with open(path, "rb") as source:
            data = source.read()
        # Réponse écrite à la main : ni Content-Length, ni keep-alive ; la fin du corps est le
        # morceau de taille 0. Un client qui borne sur la taille annoncée n'a rien à borner.
        self.wfile.write(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n")
        size = 100 if len(data) <= 200000 else 65536  # petits morceaux pour un pointeur
        for start in range(0, len(data), size):
            part = data[start:start + size]
            self.wfile.write(b"%x\r\n" % len(part) + part + b"\r\n")
        self.wfile.write(b"0\r\n\r\n")
        self.wfile.flush()
        self.close_connection = True

    def serve_slowly(self, name):
        path = os.path.join(root, os.path.basename(name))
        if not os.path.isfile(path):
            self.send_error(404)
            return
        with open(path, "rb") as source:
            data = source.read()
        self.send_response(200)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        step = max(1, len(data) // 10)
        for start in range(0, len(data), step):
            self.wfile.write(data[start:start + step])
            self.wfile.flush()
            time.sleep(0.7)

    def log_message(self, fmt, *args):
        with open("/tmp/https.log", "a", encoding="utf-8") as log:
            log.write(self.path + "\n")


os.chdir(root)
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain("/tmp/pki/cert.pem", "/tmp/pki/key.pem")
server = http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler)
server.socket = context.wrap_socket(server.socket, server_side=True)
server.serve_forever()
