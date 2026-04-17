//! Rollender Log-Buffer für den Engine-Server.
//!
//! Hält bis zu `MAX_ENTRIES` (5 000) Einträge im Speicher.
//!
//! **Persistenz-Strategie (Priorität)**
//! 1. NATS JetStream (`ENGINE_LOGS` Stream) — aktiv wenn `enable_nats()` aufgerufen wurde.
//!    Beim Start werden die letzten Einträge via `populate()` aus NATS geladen.
//! 2. Datei-Fallback via `LogBuffer::new_with_persistence(path)` — wird automatisch
//!    deaktiviert, sobald NATS verfügbar ist.
//!
//! **Datei-Persistenz-Details**
//! - Jeder neue Eintrag wird als JSON-Zeile an die Log-Datei angehängt.
//! - Nach `COMPACT_AFTER` Schreibvorgängen wird die Datei auf die letzten
//!   `MAX_ENTRIES` Zeilen kompaktiert (Temp-Datei + atomisches Rename).
//! - Beim Start werden die letzten `MAX_ENTRIES` Zeilen aus der Datei geladen.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use chrono::Utc;
use serde::Serialize;
use tracing::Level;
use tracing_subscriber::Layer;

/// Maximale Anzahl an Log-Einträgen im Buffer und in der Datei.
const MAX_ENTRIES: usize = 5_000;

/// Nach wie vielen Append-Schreibvorgängen die Datei kompaktiert wird.
/// Damit wird die Datei nie wesentlich größer als MAX_ENTRIES + COMPACT_AFTER Zeilen.
const COMPACT_AFTER: usize = 500;

// ---------------------------------------------------------------------------
// Öffentliche Typen
// ---------------------------------------------------------------------------

/// Ein einzelner Log-Eintrag.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct LogEntry {
    /// ISO-8601 Zeitstempel (UTC).
    pub timestamp: String,
    /// Log-Level: "ERROR", "WARN", "INFO", "DEBUG", "TRACE".
    pub level: String,
    /// Rust-Modul-Pfad der Quelle.
    pub target: String,
    /// Die formatierte Log-Nachricht.
    pub message: String,
}

// ---------------------------------------------------------------------------
// Interner Zustand
// ---------------------------------------------------------------------------

struct PersistState {
    path: PathBuf,
    /// Zählt Schreibvorgänge seit der letzten Kompaktierung.
    written_since_compact: usize,
}

struct InnerBuffer {
    entries: VecDeque<LogEntry>,
    persist: Option<PersistState>,
    /// NATS-Sender — wenn gesetzt, werden neue Einträge dort publiziert statt in die Datei.
    nats_tx: Option<mpsc::UnboundedSender<LogEntry>>,
}

impl Default for InnerBuffer {
    fn default() -> Self {
        Self {
            entries: VecDeque::with_capacity(MAX_ENTRIES),
            persist: None,
            nats_tx: None,
        }
    }
}

// ---------------------------------------------------------------------------
// LogBuffer
// ---------------------------------------------------------------------------

/// Rollender Log-Buffer — thread-sicher über `Arc<Mutex<...>>`.
#[derive(Debug, Clone, Default)]
pub struct LogBuffer {
    inner: Arc<Mutex<InnerBuffer>>,
}

impl std::fmt::Debug for InnerBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InnerBuffer")
            .field("entries_len", &self.entries.len())
            .field("persisted", &self.persist.is_some())
            .finish()
    }
}

impl LogBuffer {
    /// Rein in-memory Buffer (keine Datei-Persistenz). Geeignet für Tests.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(InnerBuffer {
                entries: VecDeque::with_capacity(MAX_ENTRIES),
                persist: None,
                nats_tx: None,
            })),
        }
    }

    /// Buffer mit Datei-Persistenz (Fallback wenn NATS nicht verfügbar).
    ///
    /// Beim Erstellen werden vorhandene Einträge aus `path` geladen.
    /// Jeder neue Eintrag wird an die Datei angehängt; periodisch wird
    /// kompaktiert.  Sobald `enable_nats()` aufgerufen wird, wird die
    /// Datei-Persistenz automatisch deaktiviert.
    pub fn new_with_persistence(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        let entries = load_entries_from_file(&path);
        Self {
            inner: Arc::new(Mutex::new(InnerBuffer {
                entries,
                persist: Some(PersistState {
                    path,
                    written_since_compact: 0,
                }),
                nats_tx: None,
            })),
        }
    }

    /// Verbindet den Buffer mit einem NATS-Sink.
    ///
    /// Erstellt intern den unbounded Channel, registriert ihn (deaktiviert Datei-Persistenz)
    /// und spawnt den Hintergrund-Task, der Einträge an NATS publiziert.
    /// Gibt die Anzahl der vorab aus NATS geladenen Einträge zurück.
    pub async fn attach_nats_sink(&self, sink: crate::log_nats::NatsLogSink) -> usize {
        let recent = sink.load_recent(MAX_ENTRIES).await;
        let restored = recent.len();
        self.populate(recent);

        let (tx, mut rx) = mpsc::unbounded_channel::<LogEntry>();
        {
            let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            guard.persist = None;
            guard.nats_tx = Some(tx);
        }

        tokio::spawn(async move {
            while let Some(entry) = rx.recv().await {
                sink.publish(&entry).await;
            }
        });

        restored
    }

    /// Aktiviert NATS-Persistenz über einen externen Sender (low-level, für Tests).
    pub fn enable_nats(&self, tx: mpsc::UnboundedSender<LogEntry>) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.persist = None;
        guard.nats_tx = Some(tx);
    }

    /// Füllt den In-Memory-Buffer mit den übergebenen Einträgen (z. B. beim
    /// Server-Start aus NATS geladen).  Bestehende Einträge werden ersetzt.
    pub fn populate(&self, entries: Vec<LogEntry>) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.entries.clear();
        for entry in entries {
            if guard.entries.len() >= MAX_ENTRIES {
                guard.entries.pop_front();
            }
            guard.entries.push_back(entry);
        }
    }

    /// Gibt alle Einträge zurück, optional gefiltert.
    ///
    /// - `level_filter`: Mindest-Level ("error", "warn", "info", "debug", "trace").
    /// - `search`: Substring-Filter auf `message` und `target` (case-insensitive).
    pub fn entries(&self, level_filter: Option<&str>, search: Option<&str>) -> Vec<LogEntry> {
        let min_level = level_filter
            .and_then(parse_level)
            .unwrap_or(Level::TRACE);

        let search_lower = search.map(|s| s.to_lowercase());

        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .entries
            .iter()
            .filter(|e| {
                let entry_level = parse_level(&e.level).unwrap_or(Level::TRACE);
                entry_level <= min_level
            })
            .filter(|e| {
                if let Some(ref q) = search_lower {
                    e.message.to_lowercase().contains(q.as_str())
                        || e.target.to_lowercase().contains(q.as_str())
                } else {
                    true
                }
            })
            .cloned()
            .collect()
    }

    fn push(&self, entry: LogEntry) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        // Explizit deref-en, damit der Borrow-Checker die Struct-Felder
        // `inner.persist` (mut) und `inner.entries` (immut) als disjunkt erkennt.
        let inner = &mut *guard;

        // Rollend: ältesten Eintrag verdrängen
        if inner.entries.len() >= MAX_ENTRIES {
            inner.entries.pop_front();
        }
        inner.entries.push_back(entry.clone());

        // Datei-Persistenz nur wenn kein NATS-Sender aktiv ist
        let compact_info: Option<(PathBuf, Vec<LogEntry>)> =
            if inner.nats_tx.is_none() {
                if let Some(ref mut persist) = inner.persist {
                    append_entry_to_file(&persist.path, &entry);
                    persist.written_since_compact += 1;

                    if persist.written_since_compact >= COMPACT_AFTER {
                        persist.written_since_compact = 0;
                        // Felder-Split: inner.persist (mut) + inner.entries (immut) — OK
                        let snapshot: Vec<LogEntry> = inner.entries.iter().cloned().collect();
                        Some((persist.path.clone(), snapshot))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

        // NATS-Sender klonen, bevor der Lock freigegeben wird
        let nats_tx = inner.nats_tx.clone();

        // Kompaktierung und NATS-Send außerhalb des Locks
        drop(guard);

        if let Some((path, snapshot)) = compact_info {
            compact_file(&path, &snapshot);
        }

        if let Some(tx) = nats_tx {
            // Fehler ignorieren — geschlossener Channel = Shutdown läuft
            let _ = tx.send(entry);
        }
    }
}

// ---------------------------------------------------------------------------
// Datei-I/O
// ---------------------------------------------------------------------------

/// Lädt die letzten `MAX_ENTRIES` Einträge aus einer JSON-Lines-Datei.
fn load_entries_from_file(path: &Path) -> VecDeque<LogEntry> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return VecDeque::with_capacity(MAX_ENTRIES),
    };

    let reader = BufReader::new(file);
    let mut entries: VecDeque<LogEntry> = VecDeque::with_capacity(MAX_ENTRIES);

    for line in reader.lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<LogEntry>(&line) {
            if entries.len() >= MAX_ENTRIES {
                entries.pop_front();
            }
            entries.push_back(entry);
        }
    }

    entries
}

/// Hängt einen einzelnen Eintrag als JSON-Zeile an die Datei an.
fn append_entry_to_file(path: &Path, entry: &LogEntry) {
    let Ok(mut file) = OpenOptions::new().append(true).create(true).open(path) else {
        return;
    };
    if let Ok(line) = serde_json::to_string(entry) {
        let _ = writeln!(file, "{}", line);
    }
}

/// Schreibt `entries` atomar als neue Datei (Temp-Datei + Rename).
fn compact_file(path: &Path, entries: &[LogEntry]) {
    let temp_path = path.with_extension("jsonl.tmp");
    let Ok(mut file) = File::create(&temp_path) else {
        return;
    };
    for entry in entries {
        if let Ok(line) = serde_json::to_string(entry) {
            let _ = writeln!(file, "{}", line);
        }
    }
    let _ = file.flush();
    let _ = std::fs::rename(&temp_path, path);
}

// ---------------------------------------------------------------------------
// Hilfsfunktionen
// ---------------------------------------------------------------------------

fn parse_level(s: &str) -> Option<Level> {
    match s.to_uppercase().as_str() {
        "ERROR" => Some(Level::ERROR),
        "WARN" => Some(Level::WARN),
        "INFO" => Some(Level::INFO),
        "DEBUG" => Some(Level::DEBUG),
        "TRACE" => Some(Level::TRACE),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// tracing::Layer-Implementierung
// ---------------------------------------------------------------------------

impl<S> Layer<S> for LogBuffer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let level = event.metadata().level().to_string();
        let target = event.metadata().target().to_string();

        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        self.push(LogEntry {
            timestamp: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            level,
            target,
            message: visitor.message,
        });
    }
}

/// Besucher, der das `message`-Field aus einem tracing-Event extrahiert.
#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value).trim_matches('"').to_string();
        } else if self.message.is_empty() {
            self.message = format!("{}={:?}", field.name(), value);
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }
}
