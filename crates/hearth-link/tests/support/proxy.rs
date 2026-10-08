//! Mandataire TCP à pannes, placé entre `hearth-link` et un vrai agent. Il ne regarde jamais le
//! contenu (le TLS passe tel quel : l'empreinte vue par le client est celle de l'agent).
//!
//! Pannes disponibles :
//! - `cut` : coupe net toutes les connexions et refuse les nouvelles ;
//! - `close` : ferme proprement (FIN) les connexions ouvertes, les nouvelles passent ;
//! - `freeze` : accepte mais ne transmet plus rien, dans aucun sens, sans fermer (trou noir) ;
//! - `delay` : retarde chaque morceau transmis ;
//! - `freeze_from(n)` : comme `freeze`, mais seulement à partir de la connexion numéro `n` (compte de
//!   `accepted`) : les requêtes précédentes passent, celle-là part dans le vide ;
//! - `refuse` : accepte puis ferme aussitôt les nouvelles connexions ;
//! - `heal` : tout redevient normal ; les connexions gelées ou coupées sont **jetées** (un lien
//!   gelé ne revient jamais : ce que le client y avait écrit n'arrive jamais à l'agent).
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinHandle;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Pass,
    Refuse,
    Freeze,
}

#[derive(Clone, Copy)]
struct Kill {
    generation: u64,
    graceful: bool,
}

struct Ctl {
    target: Mutex<SocketAddr>,
    mode: watch::Sender<Mode>,
    kill: watch::Sender<Kill>,
    delay_ms: AtomicU64,
    accepted: AtomicU64,
    /// Numéro (de `accepted`) à partir duquel les nouvelles connexions sont gelées ; `u64::MAX` : jamais.
    freeze_at: AtomicU64,
}

pub struct FaultProxy {
    addr: SocketAddr,
    ctl: Arc<Ctl>,
    task: JoinHandle<()>,
}

impl FaultProxy {
    pub async fn start(target: SocketAddr) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let ctl = Arc::new(Ctl {
            target: Mutex::new(target),
            mode: watch::channel(Mode::Pass).0,
            kill: watch::channel(Kill {
                generation: 0,
                graceful: false,
            })
            .0,
            delay_ms: AtomicU64::new(0),
            accepted: AtomicU64::new(0),
            freeze_at: AtomicU64::new(u64::MAX),
        });
        let task = tokio::spawn(accept_loop(listener, ctl.clone()));
        Self { addr, ctl, task }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Les nouvelles connexions vont vers `target` (agent redémarré ou réinstallé).
    pub fn set_target(&self, target: SocketAddr) {
        *self.ctl.target.lock().unwrap() = target;
    }

    /// Connexions TCP reçues depuis le démarrage.
    pub fn accepted(&self) -> u64 {
        self.ctl.accepted.load(Ordering::SeqCst)
    }

    fn kill_all(&self, graceful: bool) {
        self.ctl.kill.send_modify(|kill| {
            kill.generation += 1;
            kill.graceful = graceful;
        });
    }

    /// Coupe net : connexions ouvertes jetées, nouvelles refusées.
    pub fn cut(&self) {
        self.ctl.mode.send_replace(Mode::Refuse);
        self.kill_all(false);
    }

    /// Ferme proprement les connexions ouvertes ; les nouvelles passent.
    pub fn close(&self) {
        self.kill_all(true);
    }

    /// Refuse les nouvelles connexions, sans toucher aux ouvertes.
    pub fn refuse(&self) {
        self.ctl.mode.send_replace(Mode::Refuse);
    }

    /// Trou noir : plus rien ne passe, rien ne se ferme.
    pub fn freeze(&self) {
        self.ctl.mode.send_replace(Mode::Freeze);
    }

    /// Gèle les connexions à partir de la `n`-ième reçue depuis le démarrage (`accepted() + 1` : la
    /// prochaine). Celles d'avant passent.
    pub fn freeze_from(&self, n: u64) {
        self.ctl.freeze_at.store(n, Ordering::SeqCst);
    }

    /// Retarde chaque morceau transmis.
    pub fn delay(&self, delay: Duration) {
        self.ctl
            .delay_ms
            .store(u64::try_from(delay.as_millis()).unwrap(), Ordering::SeqCst);
    }

    /// Retour à la normale ; les connexions gelées ou coupées sont jetées.
    pub fn heal(&self) {
        self.kill_all(false);
        self.ctl.delay_ms.store(0, Ordering::SeqCst);
        self.ctl.freeze_at.store(u64::MAX, Ordering::SeqCst);
        self.ctl.mode.send_replace(Mode::Pass);
    }
}

impl Drop for FaultProxy {
    fn drop(&mut self) {
        self.task.abort();
        self.kill_all(false);
    }
}

async fn accept_loop(listener: TcpListener, ctl: Arc<Ctl>) {
    loop {
        let Ok((client, _)) = listener.accept().await else {
            return;
        };
        let number = ctl.accepted.fetch_add(1, Ordering::SeqCst) + 1;
        if number >= ctl.freeze_at.load(Ordering::SeqCst) {
            ctl.mode.send_replace(Mode::Freeze);
        }
        if *ctl.mode.borrow() == Mode::Refuse {
            drop(client);
            continue;
        }
        tokio::spawn(serve(client, ctl.clone()));
    }
}

enum End {
    Eof,
    Killed { graceful: bool },
}

async fn serve(client: TcpStream, ctl: Arc<Ctl>) {
    let generation = ctl.kill.borrow().generation;
    let target = *ctl.target.lock().unwrap();
    let Ok(upstream) = TcpStream::connect(target).await else {
        return;
    };
    let _ = client.set_nodelay(true);
    let _ = upstream.set_nodelay(true);
    let (mut client_read, mut client_write) = client.into_split();
    let (mut upstream_read, mut upstream_write) = upstream.into_split();
    let end = tokio::select! {
        end = pump(&mut client_read, &mut upstream_write, &ctl, generation) => end,
        end = pump(&mut upstream_read, &mut client_write, &ctl, generation) => end,
    };
    match end {
        End::Killed { graceful: false } => {} // jeté d'un coup
        End::Killed { graceful: true } | End::Eof => {
            let _ = client_write.shutdown().await;
            let _ = upstream_write.shutdown().await;
        }
    }
}

fn killed(ctl: &Ctl, generation: u64) -> Option<End> {
    let kill = *ctl.kill.borrow();
    (kill.generation > generation).then_some(End::Killed {
        graceful: kill.graceful,
    })
}

async fn pump(
    read: &mut (impl tokio::io::AsyncRead + Unpin),
    write: &mut (impl tokio::io::AsyncWrite + Unpin),
    ctl: &Ctl,
    generation: u64,
) -> End {
    let mut mode = ctl.mode.subscribe();
    let mut kill = ctl.kill.subscribe();
    let mut buffer = vec![0u8; 16 * 1024];
    loop {
        // Gelé : on ne lit plus rien, on attend la fin du gel ou la mort de la connexion.
        while *mode.borrow() == Mode::Freeze {
            tokio::select! {
                _ = mode.changed() => {}
                _ = kill.changed() => {}
            }
            if let Some(end) = killed(ctl, generation) {
                return end;
            }
        }
        let count = tokio::select! {
            result = read.read(&mut buffer) => match result {
                Ok(0) | Err(_) => return End::Eof,
                Ok(count) => count,
            },
            _ = kill.changed() => match killed(ctl, generation) {
                Some(end) => return end,
                None => continue,
            },
        };
        // Octets lus au moment où le gel tombe : ils ne passent pas.
        while *mode.borrow() == Mode::Freeze {
            tokio::select! {
                _ = mode.changed() => {}
                _ = kill.changed() => {}
            }
            if let Some(end) = killed(ctl, generation) {
                return end;
            }
        }
        let delay = ctl.delay_ms.load(Ordering::SeqCst);
        if delay > 0 {
            tokio::select! {
                () = tokio::time::sleep(Duration::from_millis(delay)) => {}
                _ = kill.changed() => {
                    if let Some(end) = killed(ctl, generation) {
                        return end;
                    }
                }
            }
        }
        if write.write_all(&buffer[..count]).await.is_err() {
            return End::Eof;
        }
    }
}
