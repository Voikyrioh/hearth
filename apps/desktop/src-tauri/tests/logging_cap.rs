//! Plafond de taille du journal : dernière ligne, réouverture, horloge reculée, panique sans abonné.
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

use std::cell::RefCell;
use std::io::Write;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use hearth_desktop_lib::logging::{BoundedWriter, FULL_LINE, enforce_cap, write_panic_report};

/// Écrivain témoin : garde tout ce qu'on lui envoie.
#[derive(Clone, Default)]
struct Sink(Rc<RefCell<Vec<u8>>>);

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Sink {
    fn text(&self) -> String {
        String::from_utf8(self.0.borrow().clone()).unwrap()
    }
}

fn write_log(dir: &Path, name: &str, len: usize, age_secs: u64) {
    let path = dir.join(name);
    std::fs::write(&path, vec![b'x'; len]).unwrap();
    let when = SystemTime::now() - Duration::from_secs(age_secs);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(when)
        .unwrap();
}

#[test]
fn at_the_cap_one_last_line_is_written_then_messages_are_dropped() {
    let dir = tempfile::tempdir().unwrap();
    write_log(dir.path(), "hearth.2026-10-04.log", 500, 10);
    let sink = Sink::default();
    let mut writer =
        BoundedWriter::with_recheck(sink.clone(), dir.path().to_path_buf(), 100, Duration::MAX);
    writer.write_all(b"abandonne").unwrap();
    writer.write_all(b"abandonne aussi").unwrap();
    assert_eq!(sink.text(), FULL_LINE, "une seule ligne, rien d'autre");
}

#[test]
fn the_journal_reopens_when_space_is_freed() {
    let dir = tempfile::tempdir().unwrap();
    write_log(dir.path(), "hearth.2026-10-04.log", 500, 10);
    let sink = Sink::default();
    let mut writer =
        BoundedWriter::with_recheck(sink.clone(), dir.path().to_path_buf(), 100, Duration::ZERO);
    writer.write_all(b"perdu").unwrap();
    assert!(!sink.text().contains("perdu"));
    std::fs::remove_file(dir.path().join("hearth.2026-10-04.log")).unwrap();
    writer.write_all(b"retrouve").unwrap();
    assert!(sink.text().ends_with("retrouve"), "{}", sink.text());
}

#[test]
fn a_clock_set_back_never_deletes_the_file_being_written() {
    let dir = tempfile::tempdir().unwrap();
    // Le fichier ouvert porte un nom plus ancien (horloge reculée) mais a été modifié en dernier.
    write_log(dir.path(), "hearth.2026-10-05.log", 100, 3600);
    write_log(dir.path(), "hearth.2026-10-01.log", 100, 1);
    assert!(!enforce_cap(dir.path(), 150));
    assert!(dir.path().join("hearth.2026-10-01.log").exists());
    assert!(!dir.path().join("hearth.2026-10-05.log").exists());
}

#[test]
fn a_panic_report_goes_straight_into_the_latest_log_file() {
    let dir = tempfile::tempdir().unwrap();
    write_log(dir.path(), "hearth.2026-10-03.log", 10, 100);
    write_log(dir.path(), "hearth.2026-10-04.log", 10, 1);
    assert!(write_panic_report(Some(dir.path()), "panique : boum\n"));
    let latest = std::fs::read_to_string(dir.path().join("hearth.2026-10-04.log")).unwrap();
    assert!(latest.contains("boum"));
    let older = std::fs::read_to_string(dir.path().join("hearth.2026-10-03.log")).unwrap();
    assert!(!older.contains("boum"));
}

#[test]
fn a_panic_report_creates_a_file_when_there_is_no_journal_yet() {
    let dir = tempfile::tempdir().unwrap();
    assert!(write_panic_report(Some(dir.path()), "panique : boum\n"));
    assert!(dir.path().join("hearth.panic.log").exists());
}

#[test]
fn an_unavailable_journal_is_reported_so_the_panic_can_be_shown() {
    assert!(!write_panic_report(None, "x"));
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("fichier");
    std::fs::write(&blocker, "x").unwrap();
    assert!(!write_panic_report(Some(&blocker.join("logs")), "x"));
}

#[test]
fn the_cap_sees_the_real_size_of_a_file_that_is_still_open() {
    let dir = tempfile::tempdir().unwrap();
    write_log(dir.path(), "hearth.2026-10-04.log", 100, 100);
    // Le fichier du jour reste ouvert en écriture pendant le calcul (comme l'appender).
    let mut current = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.path().join("hearth.2026-10-05.log"))
        .unwrap();
    current.write_all(&[b'x'; 1000]).unwrap();
    assert!(
        enforce_cap(dir.path(), 500),
        "1000 octets dépassent le plafond"
    );
    assert!(!dir.path().join("hearth.2026-10-04.log").exists());
    assert!(dir.path().join("hearth.2026-10-05.log").exists());
    current.write_all(b"encore").unwrap();
}
