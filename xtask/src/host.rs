//! Le monde extérieur de `release-stage` et `release-upload` (HRT-51) : lancer un programme (arguments
//! en liste, jamais par un interpréteur), lire l'horloge, attendre. Injecté partout : les tests le
//! remplacent par un faux qui enregistre les appels, sans réseau, sans `aws`, sans `git`.

use std::io::Write;
use std::time::{Duration, Instant};

/// Résultat d'une commande externe.
#[derive(Debug, Clone)]
pub struct CmdOutput {
    pub success: bool,
    /// Code de sortie (`None` : tué par un signal). `aws` rend 254 pour une erreur renvoyée par le
    /// service (documentation des codes de retour de l'AWS CLI v2).
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

/// Une commande à lancer : programme, arguments en liste, entrée standard facultative, variables
/// d'environnement ajoutées (jamais de secret : les identifiants viennent de l'environnement du
/// processus parent, lus par le programme lui-même).
#[derive(Debug, Clone, Copy)]
pub struct Call<'a> {
    pub program: &'a str,
    pub args: &'a [String],
    pub input: Option<&'a [u8]>,
    pub env: &'a [(&'a str, &'a str)],
}

impl<'a> Call<'a> {
    pub fn new(program: &'a str, args: &'a [String]) -> Self {
        Self {
            program,
            args,
            input: None,
            env: &[],
        }
    }

    pub fn input(mut self, input: &'a [u8]) -> Self {
        self.input = Some(input);
        self
    }

    pub fn env(mut self, env: &'a [(&'a str, &'a str)]) -> Self {
        self.env = env;
        self
    }
}

/// Tout ce que ces tâches demandent au monde extérieur.
pub trait Host {
    /// `Err` = le programme n'a pas pu être lancé ; un échec du programme est un `CmdOutput` non réussi.
    fn run(&self, call: &Call) -> Result<CmdOutput, String>;
    fn elapsed(&self) -> Duration;
    fn sleep(&self, duration: Duration);
}

/// Le vrai monde : processus du système, horloge, `thread::sleep`.
pub struct System {
    start: Instant,
}

impl System {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Host for System {
    fn run(&self, call: &Call) -> Result<CmdOutput, String> {
        let program = call.program;
        let mut command = std::process::Command::new(program);
        command
            .args(call.args)
            .envs(call.env.iter().copied())
            .stdin(if call.input.is_some() {
                std::process::Stdio::piped()
            } else {
                std::process::Stdio::null()
            })
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                format!(
                    "« {program} » est introuvable : installe-le et mets-le dans le PATH (AWS CLI v2 pour aws ; curl est livré avec Windows 10 et suivants)"
                )
            } else {
                format!("{program} : {error}")
            }
        })?;
        // L'entrée est écrite dans un fil à part : le programme peut remplir sa sortie avant d'avoir tout lu.
        let writer = match (call.input, child.stdin.take()) {
            (Some(bytes), Some(mut stdin)) => {
                let data = bytes.to_vec();
                Some(std::thread::spawn(move || stdin.write_all(&data)))
            }
            _ => None,
        };
        let output = child
            .wait_with_output()
            .map_err(|error| format!("{program} : {error}"))?;
        if let Some(writer) = writer {
            match writer.join() {
                Ok(Ok(())) => {}
                // Le programme s'est arrêté avant d'avoir tout lu : son code de sortie le dira.
                Ok(Err(error)) if error.kind() == std::io::ErrorKind::BrokenPipe => {}
                Ok(Err(error)) => {
                    return Err(format!(
                        "{program} : écriture de l'entrée impossible : {error}"
                    ));
                }
                Err(_) => return Err(format!("{program} : écriture de l'entrée interrompue")),
            }
        }
        Ok(CmdOutput {
            success: output.status.success(),
            code: output.status.code(),
            stdout: output.stdout,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }

    fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    /// `System::run` est le seul code qui parle au vrai monde : on l'essaie sur `git`, présent partout
    /// où ce dépôt se construit, et inoffensif (lecture seule, rien n'est écrit).
    #[test]
    fn system_run_launches_a_real_program_and_reports_its_output() {
        let list = args(&["--version"]);
        let output = System::new().run(&Call::new("git", &list)).unwrap();
        assert!(output.success && output.code == Some(0));
        assert!(
            String::from_utf8_lossy(&output.stdout).starts_with("git version"),
            "{:?}",
            output.stdout
        );
    }

    #[test]
    fn system_run_reports_a_failing_exit_code_and_the_message() {
        let list = args(&["rev-parse", "--verify", "hearth-ref-inexistante"]);
        let output = System::new().run(&Call::new("git", &list)).unwrap();
        assert!(!output.success);
        assert!(output.code.is_some_and(|code| code != 0));
        assert!(!output.stderr.is_empty());
    }

    /// L'entrée standard arrive intacte (octets, pas de traduction de fins de ligne) : `git hash-object
    /// --stdin` n'écrit rien et rend l'empreinte de ce qu'il a reçu (celle, connue, de « test content\n »).
    #[test]
    fn system_run_feeds_the_input_bytes_unchanged() {
        let list = args(&["hash-object", "--stdin"]);
        let call = Call::new("git", &list).input(b"test content\n");
        let output = System::new().run(&call).unwrap();
        assert!(output.success, "{}", output.stderr);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "d670460b4b4aece5915caf5c68d12f560a9fe3e4"
        );
    }

    #[test]
    fn system_run_sets_the_given_variables_on_the_process_only() {
        let list = args(&["var", "GIT_AUTHOR_IDENT"]);
        let env = [
            ("GIT_AUTHOR_NAME", "Hearth Test"),
            ("GIT_AUTHOR_EMAIL", "test@example.invalid"),
        ];
        let output = System::new()
            .run(&Call::new("git", &list).env(&env))
            .unwrap();
        assert!(output.success, "{}", output.stderr);
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("Hearth Test"),
            "{:?}",
            output.stdout
        );
    }

    #[test]
    fn system_run_names_a_missing_program() {
        let list = args(&[]);
        let error = System::new()
            .run(&Call::new("hearth-programme-qui-n-existe-pas", &list))
            .unwrap_err();
        assert!(error.contains("introuvable"), "{error}");
    }
}
