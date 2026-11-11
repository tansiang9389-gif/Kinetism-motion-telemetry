//! Offline-first data store: local JSON cache + Firestore sync worker.
//!
//! Every change is applied locally and cached immediately, then queued for
//! Firestore. Pending writes survive restarts and are retried until the
//! backend accepts them. Remote data is pulled on start and every 5 minutes so
//! records captured on the phone app show up here.

pub mod firestore;

use crate::clinical::Side;
use crate::model::{GameSession, MeasurementRecord, Patient};
use chrono::{DateTime, Utc};
use firestore::{Fields, FirestoreClient};
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// A pull reads every document of all three collections, so it stays rare:
/// the free plan allows 50,000 reads a day, shared with the phone app.
const PULL_EVERY: Duration = Duration::from_secs(5 * 60);
/// Failed writes are retried on their own, much shorter, timer.
const RETRY_EVERY: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PendingWrite {
    pub collection: String,
    pub id: String,
    pub fields: Fields,
    pub merge: bool,
    /// Queue order. An acknowledgement only clears the write it was sent for,
    /// never a newer write for the same document queued behind it.
    #[serde(default)]
    pub seq: u64,
    #[serde(skip)]
    pub in_flight: bool,
}

impl PendingWrite {
    fn is(&self, collection: &str, id: &str, seq: u64) -> bool {
        self.collection == collection && self.id == id && self.seq == seq
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub selected_patient: Option<String>,
    pub voice: bool,
    pub physio_unlock_all: bool,
    pub side: Side,
    pub reps_per_round: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            selected_patient: None,
            voice: true,
            physio_unlock_all: false,
            side: Side::Right,
            reps_per_round: 8,
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct CacheFile {
    patients: Vec<Patient>,
    records: Vec<MeasurementRecord>,
    games: Vec<GameSession>,
    pending: Vec<PendingWrite>,
    settings: Settings,
}

#[derive(Clone, Debug, Default)]
pub struct SyncStatus {
    pub enabled: bool,
    /// A pull is in flight; no other pull starts until it ends.
    pub loading: bool,
    pub last_pull: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

enum Request {
    Refresh,
    Write(PendingWrite),
}

enum Event {
    Pulled {
        patients: Vec<Patient>,
        records: Vec<MeasurementRecord>,
        games: Vec<GameSession>,
    },
    Written {
        collection: String,
        id: String,
        seq: u64,
    },
    WriteFailed {
        collection: String,
        id: String,
        seq: u64,
        message: String,
    },
    PullFailed(String),
}

pub struct DataStore {
    pub patients: Vec<Patient>,
    pub records: Vec<MeasurementRecord>,
    pub games: Vec<GameSession>,
    pub settings: Settings,
    pending: Vec<PendingWrite>,
    /// Sequence number for the next queued write.
    next_seq: u64,
    status: SyncStatus,
    cache_path: PathBuf,
    requests: Option<UnboundedSender<Request>>,
    events: Option<UnboundedReceiver<Event>>,
    ctx: eframe::egui::Context,
    last_pull: Instant,
    last_retry: Instant,
    cache_error: Option<String>,
    /// Problem found while loading the cache; shown for the whole session.
    cache_warning: Option<String>,
    /// The existing cache could not be read or moved aside: never overwrite it.
    save_blocked: bool,
}

pub fn data_dir() -> PathBuf {
    std::env::var_os("KINETISM_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| d.join("Kinetism")))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Cache file for a sync target (`None` = local only). Every target keeps its
/// own file, so writes queued offline or for a test project are never flushed
/// to the shared project on a later run.
pub fn cache_file_name(project_id: Option<&str>) -> String {
    let Some(project) = project_id else {
        return "desktop-cache-offline.json".into();
    };
    if project == firestore::DEFAULT_PROJECT_ID {
        return "desktop-cache.json".into();
    }
    let safe: String = project
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    // Real project ids never contain '_', so this cannot clash with another project.
    let safe = if safe == "offline" {
        "_offline".to_string()
    } else {
        safe
    };
    format!("desktop-cache-{safe}.json")
}

/// Loaded cache, a warning for the status bar, and whether saving must stay off.
type LoadedCache = (CacheFile, Option<String>, bool);

/// Reads the offline cache. A cache that exists but cannot be read or parsed is
/// moved aside first, so the next save cannot overwrite unsynced records.
fn load_cache(path: &Path) -> LoadedCache {
    let problem = match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<CacheFile>(&text) {
            Ok(cache) => return (cache, None, false),
            Err(e) => e.to_string(),
        },
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            return (CacheFile::default(), None, false);
        }
        Err(e) => e.to_string(),
    };
    let backup = path.with_extension(format!(
        "unreadable-{}.json",
        Utc::now().format("%Y%m%d-%H%M%S")
    ));
    match std::fs::rename(path, &backup) {
        Ok(()) => (
            CacheFile::default(),
            Some(format!(
                "Local cache unreadable ({problem}); it was moved to {} and the app started from Firestore data.",
                backup.display()
            )),
            false,
        ),
        Err(e) => (
            CacheFile::default(),
            Some(format!(
                "Local cache unreadable ({problem}) and could not be moved aside ({e}). Local saving is paused so it is not overwritten; close other programs using it and restart."
            )),
            true,
        ),
    }
}

impl DataStore {
    /// `project_id = None` keeps everything local (used for demos and tests).
    /// `cache_path` is normally `data_dir()` joined with [`cache_file_name`].
    pub fn open(
        runtime: &tokio::runtime::Handle,
        ctx: eframe::egui::Context,
        project_id: Option<String>,
        cache_path: PathBuf,
    ) -> Self {
        let (cache, cache_warning, save_blocked) = load_cache(&cache_path);
        let next_seq = cache.pending.iter().map(|w| w.seq).max().unwrap_or(0) + 1;
        let mut store = Self {
            patients: cache.patients,
            records: cache.records,
            games: cache.games,
            settings: cache.settings,
            pending: cache.pending,
            next_seq,
            status: SyncStatus {
                enabled: project_id.is_some(),
                ..SyncStatus::default()
            },
            cache_path,
            requests: None,
            events: None,
            ctx,
            last_pull: Instant::now(),
            last_retry: Instant::now(),
            cache_error: None,
            cache_warning,
            save_blocked,
        };
        if let Some(project) = project_id {
            let (req_tx, req_rx) = unbounded_channel();
            let (ev_tx, ev_rx) = unbounded_channel();
            runtime.spawn(worker(
                FirestoreClient::new(&project),
                req_rx,
                ev_tx,
                store.ctx.clone(),
            ));
            store.requests = Some(req_tx);
            store.events = Some(ev_rx);
            store.refresh();
            store.flush_pending();
        }
        store.sort();
        store
    }

    pub fn status(&self) -> &SyncStatus {
        &self.status
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn cache_error(&self) -> Option<&str> {
        self.cache_error
            .as_deref()
            .or(self.cache_warning.as_deref())
    }

    /// Pull the shared collections now, unless a pull is still running.
    pub fn refresh(&mut self) {
        if self.status.loading {
            return;
        }
        if let Some(tx) = &self.requests
            && tx.send(Request::Refresh).is_ok()
        {
            self.status.loading = true;
        }
        self.last_pull = Instant::now();
    }

    fn flush_pending(&mut self) {
        let Some(tx) = &self.requests else {
            return;
        };
        for write in self.pending.iter_mut().filter(|w| !w.in_flight) {
            write.in_flight = true;
            let _ = tx.send(Request::Write(write.clone()));
        }
        self.last_retry = Instant::now();
    }

    /// Process worker events; call once per frame.
    pub fn poll(&mut self) {
        self.poll_at(Instant::now());
    }

    fn poll_at(&mut self, now: Instant) {
        let mut changed = false;
        let mut events = Vec::new();
        if let Some(rx) = &mut self.events {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }
        for event in events {
            match event {
                Event::Pulled {
                    patients,
                    records,
                    games,
                } => {
                    self.merge_remote(patients, records, games);
                    self.status.loading = false;
                    self.status.last_pull = Some(Utc::now());
                    self.status.last_error = None;
                    changed = true;
                }
                Event::Written {
                    collection,
                    id,
                    seq,
                } => {
                    self.pending.retain(|w| !w.is(&collection, &id, seq));
                    if self.pending.is_empty() {
                        self.status.last_error = None;
                    }
                    changed = true;
                }
                Event::WriteFailed {
                    collection,
                    id,
                    seq,
                    message,
                } => {
                    if let Some(w) = self
                        .pending
                        .iter_mut()
                        .find(|w| w.is(&collection, &id, seq))
                    {
                        w.in_flight = false;
                    }
                    self.status.last_error = Some(message);
                }
                Event::PullFailed(message) => {
                    self.status.loading = false;
                    self.status.last_error = Some(message);
                }
            }
        }
        if self.status.enabled {
            if now.saturating_duration_since(self.last_pull) > PULL_EVERY {
                self.refresh();
            }
            if now.saturating_duration_since(self.last_retry) > RETRY_EVERY {
                self.flush_pending();
            }
        }
        if changed {
            self.save_cache();
        }
    }

    fn is_pending(&self, collection: &str, id: &str) -> bool {
        self.pending
            .iter()
            .any(|w| w.collection == collection && w.id == id)
    }

    fn merge_remote(
        &mut self,
        patients: Vec<Patient>,
        records: Vec<MeasurementRecord>,
        games: Vec<GameSession>,
    ) {
        merge(
            &mut self.patients,
            patients,
            &self.pending,
            firestore::PATIENTS,
            |p| &p.id,
        );
        merge(
            &mut self.records,
            records,
            &self.pending,
            firestore::RECORDS,
            |r| &r.id,
        );
        merge(
            &mut self.games,
            games,
            &self.pending,
            firestore::GAMES,
            |g| &g.id,
        );
        self.sort();
    }

    fn sort(&mut self) {
        self.patients
            .sort_by_key(|p| std::cmp::Reverse(p.updated_at));
        self.records
            .sort_by_key(|r| std::cmp::Reverse(r.recorded_at));
        self.games.sort_by_key(|g| std::cmp::Reverse(g.recorded_at));
    }

    fn enqueue(&mut self, collection: &str, id: &str, fields: Fields, merge: bool) {
        self.pending
            .retain(|w| !(w.collection == collection && w.id == id));
        let seq = self.next_seq;
        self.next_seq += 1;
        self.pending.push(PendingWrite {
            collection: collection.into(),
            id: id.into(),
            fields,
            merge,
            seq,
            in_flight: false,
        });
        self.flush_pending();
        self.save_cache();
    }

    pub fn upsert_patient(&mut self, patient: Patient) {
        let fields = firestore::patient_to_fields(&patient);
        let id = patient.id.clone();
        match self.patients.iter_mut().find(|p| p.id == id) {
            Some(existing) => *existing = patient,
            None => self.patients.insert(0, patient),
        }
        self.enqueue(firestore::PATIENTS, &id, fields, true);
    }

    pub fn add_record(&mut self, record: MeasurementRecord) {
        let fields = firestore::record_to_fields(&record);
        let id = record.id.clone();
        self.records.retain(|r| r.id != id);
        self.records.insert(0, record);
        self.enqueue(firestore::RECORDS, &id, fields, false);
    }

    pub fn add_game(&mut self, game: GameSession) {
        let fields = firestore::game_to_fields(&game);
        let id = game.id.clone();
        self.games.insert(0, game);
        self.enqueue(firestore::GAMES, &id, fields, false);
    }

    pub fn is_record_pending(&self, id: &str) -> bool {
        self.is_pending(firestore::RECORDS, id)
    }

    pub fn save_cache(&mut self) {
        if self.save_blocked {
            return;
        }
        let cache = CacheFile {
            patients: self.patients.clone(),
            records: self.records.clone(),
            games: self.games.clone(),
            pending: self.pending.clone(),
            settings: self.settings.clone(),
        };
        let result = (|| -> std::io::Result<()> {
            if let Some(parent) = self.cache_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let tmp = self.cache_path.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_vec_pretty(&cache)?)?;
            std::fs::rename(&tmp, &self.cache_path)
        })();
        self.cache_error = result
            .err()
            .map(|e| format!("Could not save the local cache: {e}"));
    }

    pub fn patient(&self, id: &str) -> Option<&Patient> {
        self.patients.iter().find(|p| p.id == id)
    }

    /// A measurement record, only if it belongs to `patient_id`.
    pub fn patient_record(&self, patient_id: &str, id: &str) -> Option<&MeasurementRecord> {
        self.records
            .iter()
            .find(|r| r.id == id && r.patient_id == patient_id)
    }

    pub fn records_for(&self, patient_id: &str) -> Vec<&MeasurementRecord> {
        self.records
            .iter()
            .filter(|r| r.patient_id == patient_id)
            .collect()
    }

    pub fn games_for(&self, patient_id: &str) -> Vec<&GameSession> {
        self.games
            .iter()
            .filter(|g| g.patient_id == patient_id)
            .collect()
    }
}

/// Pulled copies replace local ones, except items whose local write is still
/// queued (e.g. a physio grade not yet acknowledged): those win.
fn merge<T>(
    local: &mut Vec<T>,
    remote: Vec<T>,
    pending: &[PendingWrite],
    collection: &str,
    id: impl Fn(&T) -> &str,
) {
    let queued: Vec<T> = local
        .drain(..)
        .filter(|item| {
            pending
                .iter()
                .any(|w| w.collection == collection && w.id == id(item))
        })
        .collect();
    *local = remote;
    for item in queued {
        match local.iter_mut().find(|x| id(x) == id(&item)) {
            Some(existing) => *existing = item,
            None => local.push(item),
        }
    }
}

async fn worker(
    client: FirestoreClient,
    mut requests: UnboundedReceiver<Request>,
    events: UnboundedSender<Event>,
    ctx: eframe::egui::Context,
) {
    while let Some(request) = requests.recv().await {
        let event = match request {
            Request::Refresh => pull(&client).await,
            Request::Write(w) => match client
                .upsert(&w.collection, &w.id, &w.fields, w.merge)
                .await
            {
                Ok(()) => Event::Written {
                    collection: w.collection,
                    id: w.id,
                    seq: w.seq,
                },
                Err(e) => Event::WriteFailed {
                    collection: w.collection,
                    id: w.id,
                    seq: w.seq,
                    message: if e.offline {
                        format!("{} - will retry", e.message)
                    } else {
                        format!("{} - check Firestore rules", e.message)
                    },
                },
            },
        };
        if events.send(event).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}

async fn pull(client: &FirestoreClient) -> Event {
    let patients = match client.list(firestore::PATIENTS).await {
        Ok(docs) => docs
            .iter()
            .map(|(id, f)| firestore::patient_from_fields(id, f))
            .collect(),
        Err(e) => return Event::PullFailed(e.message),
    };
    let records = match client.list(firestore::RECORDS).await {
        Ok(docs) => docs
            .iter()
            .map(|(id, f)| firestore::record_from_fields(id, f))
            .collect(),
        Err(e) => return Event::PullFailed(e.message),
    };
    let games = match client.list(firestore::GAMES).await {
        Ok(docs) => docs
            .iter()
            .filter_map(|(id, f)| firestore::game_from_fields(id, f))
            .collect(),
        Err(e) => return Event::PullFailed(e.message),
    };
    Event::Pulled {
        patients,
        records,
        games,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clinical::{Anthropometrics, Movement};
    use crate::model::{GameLevel, MmtSource};
    use std::path::Path;

    fn write(collection: &str, id: &str) -> PendingWrite {
        PendingWrite {
            collection: collection.into(),
            id: id.into(),
            fields: Fields::new(),
            merge: false,
            seq: 0,
            in_flight: true,
        }
    }

    #[test]
    fn queued_local_edits_win_over_pulled_copies() {
        let mut local = vec![
            ("a".to_string(), 4),
            ("b".to_string(), 1),
            ("new".to_string(), 2),
        ];
        let remote = vec![
            ("a".to_string(), 3),
            ("b".to_string(), 9),
            ("c".to_string(), 5),
        ];
        let pending = [write("X", "a"), write("X", "new"), write("Y", "b")];
        merge(&mut local, remote, &pending, "X", |item| &item.0);
        local.sort();
        assert_eq!(
            local,
            vec![
                ("a".to_string(), 4),
                ("b".to_string(), 9),
                ("c".to_string(), 5),
                ("new".to_string(), 2)
            ]
        );
    }

    /// Scratch folder under the git-ignored `target` directory.
    fn test_dir(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-data")
            .join(crate::model::new_id(name))
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime")
    }

    /// Local-only store: no project id, so nothing ever talks to Firestore.
    fn open(dir: &Path, runtime: &tokio::runtime::Runtime) -> DataStore {
        DataStore::open(
            runtime.handle(),
            eframe::egui::Context::default(),
            None,
            dir.join("desktop-cache.json"),
        )
    }

    fn patient(id: &str, name: &str) -> Patient {
        Patient {
            id: id.into(),
            name: name.into(),
            code: "PT-101".into(),
            arm: None,
            affected_side: Some(Side::Left),
            updated_at: Some(Utc::now()),
        }
    }

    fn record(id: &str, grade: u8, recorded_at: Option<DateTime<Utc>>) -> MeasurementRecord {
        MeasurementRecord {
            id: id.into(),
            patient_id: "p1".into(),
            patient_name: "Siti".into(),
            body_part: "Shoulder".into(),
            movement: Movement::ShoulderAbduction,
            side: Some(Side::Left),
            arm: Anthropometrics::default(),
            angle_degrees: 40.0,
            peak_angle_degrees: Some(42.0),
            force_newtons: 20.0,
            torque_nm: Some(4.0),
            mmt_grade: grade,
            mmt_title: crate::clinical::mmt_title(grade).into(),
            mmt_rationale: "test".into(),
            mmt_source: MmtSource::Auto,
            auto_mmt_grade: Some(grade),
            attempt_number: 1,
            elbow_flexion_degrees: None,
            plane_degrees: None,
            recorded_at,
            source: crate::model::SOURCE_DESKTOP.into(),
        }
    }

    fn game(id: &str) -> GameSession {
        GameSession {
            id: id.into(),
            patient_id: "p1".into(),
            patient_name: "Siti".into(),
            side: Side::Left,
            level: GameLevel::AppleTrace,
            score_percent: 50.0,
            points: 100,
            stars: 0,
            reps: 3,
            clean_reps: 0,
            bombs_hit: 0,
            max_combo: 0,
            accuracy_percent: 50.0,
            completion_percent: 50.0,
            duration_seconds: 60.0,
            max_elevation_deg: 45.0,
            recorded_at: Some(Utc::now()),
            source: crate::model::SOURCE_DESKTOP.into(),
        }
    }

    #[test]
    fn offline_writes_are_deduplicated_and_survive_a_restart() {
        let runtime = runtime();
        let dir = test_dir("restart");
        let mut store = open(&dir, &runtime);
        assert!(
            !store.status().enabled,
            "no project id keeps the store local"
        );
        assert!(
            store.patients.is_empty() && store.cache_error().is_none(),
            "a missing cache is not an error"
        );

        let mut siti = patient("p1", "Siti");
        store.upsert_patient(siti.clone());
        siti.name = "Siti Aminah".into();
        store.upsert_patient(siti.clone());
        let yesterday = Utc::now() - chrono::Duration::days(1);
        store.add_record(record("r-old", 2, Some(yesterday)));
        store.add_record(record("r-new", 3, Some(Utc::now())));
        store.add_record(record("r-undated", 1, None));
        let mut regraded = record("r-new", 4, Some(Utc::now()));
        regraded.mmt_source = MmtSource::Physio;
        store.add_record(regraded);
        store.add_game(game("g1"));
        assert_eq!(store.patients.len(), 1, "editing a patient replaces it");
        assert_eq!(store.records.len(), 3, "re-grading replaces the record");
        assert_eq!(store.pending_count(), 5, "one queued write per document");
        assert!(store.is_record_pending("r-new") && !store.is_record_pending("r-missing"));
        let queued = store
            .pending
            .iter()
            .find(|w| w.id == "p1")
            .expect("patient write");
        assert_eq!(
            queued.fields["name"],
            firestore::string("Siti Aminah"),
            "the latest edit is what gets sent"
        );

        // Writes that were in flight when the app closed must be retried.
        store.pending.iter_mut().for_each(|w| w.in_flight = true);
        store.settings.reps_per_round = 5;
        store.save_cache();
        assert!(store.cache_error().is_none(), "{:?}", store.cache_error());

        let reopened = open(&dir, &runtime);
        assert_eq!(reopened.patients, store.patients);
        assert_eq!(reopened.settings.reps_per_round, 5);
        assert_eq!(
            reopened.pending_count(),
            5,
            "unsent writes survive a restart"
        );
        assert!(
            reopened.pending.iter().all(|w| !w.in_flight),
            "restored writes are sent again"
        );
        let ids: Vec<&str> = reopened.records.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            ["r-new", "r-old", "r-undated"],
            "newest first, undated last"
        );
        assert_eq!(reopened.records[0].mmt_source, MmtSource::Physio);
        assert_eq!(reopened.records_for("p1").len(), 3);
        assert!(reopened.records_for("p2").is_empty());
        assert_eq!(reopened.games_for("p1").len(), 1);
        assert_eq!(
            reopened.patient("p1").map(|p| p.name.as_str()),
            Some("Siti Aminah")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn sent(requests: &mut UnboundedReceiver<Request>) -> Vec<String> {
        std::iter::from_fn(|| requests.try_recv().ok())
            .map(|r| match r {
                Request::Refresh => "refresh".to_string(),
                Request::Write(w) => w.id,
            })
            .collect()
    }

    fn deliver(events: &UnboundedSender<Event>, event: Event) {
        assert!(events.send(event).is_ok(), "the store is listening");
    }

    /// Sequence number of the queued write for record `id`.
    fn seq_of(store: &DataStore, id: &str) -> u64 {
        store
            .pending
            .iter()
            .find(|w| w.collection == firestore::RECORDS && w.id == id)
            .map(|w| w.seq)
            .expect("a queued write")
    }

    #[test]
    fn sync_events_retry_failed_writes_and_merge_pulled_data() {
        let runtime = runtime();
        let dir = test_dir("sync");
        let mut store = open(&dir, &runtime);
        // Stand in for the Firestore worker: capture requests, inject events.
        let (req_tx, mut requests) = unbounded_channel();
        let (events, ev_rx) = unbounded_channel();
        store.requests = Some(req_tx);
        store.events = Some(ev_rx);
        store.status.enabled = true;

        store.add_record(record("r1", 2, Some(Utc::now())));
        store.add_record(record("r2", 3, Some(Utc::now())));
        assert_eq!(
            sent(&mut requests),
            ["r1", "r2"],
            "writes are sent when queued"
        );

        let offline = "Cannot reach Firestore (timeout) - will retry";
        deliver(
            &events,
            Event::WriteFailed {
                collection: firestore::RECORDS.into(),
                id: "r1".into(),
                seq: seq_of(&store, "r1"),
                message: offline.into(),
            },
        );
        deliver(
            &events,
            Event::Written {
                collection: firestore::RECORDS.into(),
                id: "r2".into(),
                seq: seq_of(&store, "r2"),
            },
        );
        store.poll();
        assert_eq!(
            store.pending_count(),
            1,
            "the acknowledged write leaves the queue"
        );
        assert_eq!(
            store.status().last_error.as_deref(),
            Some(offline),
            "the error stays while writes are queued"
        );
        store.flush_pending();
        assert_eq!(
            sent(&mut requests),
            ["r1"],
            "only the failed write is retried"
        );

        store.poll_at(Instant::now() + PULL_EVERY + Duration::from_secs(1));
        assert_eq!(
            sent(&mut requests),
            ["refresh"],
            "the periodic refresh pulls without duplicating in-flight writes"
        );
        assert!(store.status().loading);

        let denied = "Firestore 403 PERMISSION_DENIED: Missing or insufficient permissions.";
        deliver(&events, Event::PullFailed(denied.into()));
        store.poll();
        assert!(!store.status().loading);
        assert_eq!(store.status().last_error.as_deref(), Some(denied));

        let mut remote_r1 = record("r1", 0, Some(Utc::now()));
        remote_r1.mmt_title = "remote copy".into();
        let mut phone = record("r9", 5, Some(Utc::now()));
        phone.patient_id = "p9".into();
        deliver(
            &events,
            Event::Pulled {
                patients: vec![patient("p9", "Phone patient")],
                records: vec![remote_r1, phone],
                games: Vec::new(),
            },
        );
        store.poll();
        assert!(store.status().last_pull.is_some() && !store.status().loading);
        assert!(
            store.status().last_error.is_none(),
            "a good pull clears the error"
        );
        let r1 = store.records.iter().find(|r| r.id == "r1").expect("r1");
        assert_eq!(
            r1.mmt_grade, 2,
            "a queued local edit wins over the pulled copy"
        );
        assert!(
            store.records.iter().any(|r| r.id == "r9"),
            "records from the phone appear"
        );
        assert!(
            !store.records.iter().any(|r| r.id == "r2"),
            "records that are no longer queued follow the pulled data"
        );
        assert_eq!(store.patients.len(), 1);

        deliver(
            &events,
            Event::WriteFailed {
                collection: firestore::RECORDS.into(),
                id: "r1".into(),
                seq: seq_of(&store, "r1"),
                message: offline.into(),
            },
        );
        store.poll();
        store.flush_pending();
        assert_eq!(sent(&mut requests), ["r1"]);
        deliver(
            &events,
            Event::Written {
                collection: firestore::RECORDS.into(),
                id: "r1".into(),
                seq: seq_of(&store, "r1"),
            },
        );
        store.poll();
        assert_eq!(store.pending_count(), 0);
        assert!(
            store.status().last_error.is_none(),
            "an empty queue clears the error"
        );
        let reopened = open(&dir, &runtime);
        assert_eq!(reopened.pending_count(), 0, "the drained queue was saved");
        assert!(reopened.records.iter().any(|r| r.id == "r9"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Wire a local store to a stand-in Firestore worker: requests are
    /// captured and events injected, so nothing touches the network.
    fn synced(store: &mut DataStore) -> (UnboundedReceiver<Request>, UnboundedSender<Event>) {
        let (req_tx, requests) = unbounded_channel();
        let (events, ev_rx) = unbounded_channel();
        store.requests = Some(req_tx);
        store.events = Some(ev_rx);
        store.status.enabled = true;
        (requests, events)
    }

    fn sent_writes(requests: &mut UnboundedReceiver<Request>) -> Vec<PendingWrite> {
        std::iter::from_fn(|| requests.try_recv().ok())
            .map(|r| match r {
                Request::Write(w) => w,
                Request::Refresh => panic!("no pull expected"),
            })
            .collect()
    }

    #[test]
    fn an_old_acknowledgement_never_drops_a_newer_write_for_the_same_record() {
        let runtime = runtime();
        let dir = test_dir("seq");
        let mut store = open(&dir, &runtime);
        let (mut requests, events) = synced(&mut store);
        store.add_record(record("r1", 3, Some(Utc::now())));
        let grade3 = seq_of(&store, "r1");
        let mut physio = record("r1", 4, Some(Utc::now()));
        physio.mmt_source = MmtSource::Physio;
        store.add_record(physio);
        let grade4 = seq_of(&store, "r1");
        assert!(grade4 > grade3, "every queued write gets a new number");
        let both: Vec<u64> = sent_writes(&mut requests).iter().map(|w| w.seq).collect();
        assert_eq!(
            both,
            [grade3, grade4],
            "the physio's grade 4 follows grade 3"
        );

        deliver(
            &events,
            Event::Written {
                collection: firestore::RECORDS.into(),
                id: "r1".into(),
                seq: grade3,
            },
        );
        store.poll();
        assert_eq!(
            (store.pending_count(), seq_of(&store, "r1")),
            (1, grade4),
            "saving grade 3 does not clear the queued grade 4"
        );
        store.flush_pending();
        assert!(
            sent_writes(&mut requests).is_empty(),
            "grade 4 is still in flight"
        );

        deliver(
            &events,
            Event::WriteFailed {
                collection: firestore::RECORDS.into(),
                id: "r1".into(),
                seq: grade4,
                message: "Cannot reach Firestore (timeout) - will retry".into(),
            },
        );
        store.poll();
        assert!(store.is_record_pending("r1"), "the correction stays queued");
        assert!(
            store.pending.iter().all(|w| !w.in_flight),
            "and is no longer in flight"
        );
        store.flush_pending();
        let retried = sent_writes(&mut requests);
        assert_eq!(retried.len(), 1);
        assert_eq!(
            (retried[0].seq, &retried[0].fields["mmtGrade"]),
            (grade4, &firestore::integer(4)),
            "the retry sends the physio's grade"
        );

        drop(store);
        let mut reopened = open(&dir, &runtime);
        assert_eq!(
            seq_of(&reopened, "r1"),
            grade4,
            "the number survives a restart"
        );
        reopened.add_record(record("r2", 2, None));
        assert!(
            seq_of(&reopened, "r2") > grade4,
            "numbering continues above the cached writes"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pulls_are_rare_and_never_overlap_while_failed_writes_retry_promptly() {
        let runtime = runtime();
        let dir = test_dir("timers");
        let mut store = open(&dir, &runtime);
        let (mut requests, events) = synced(&mut store);
        let later = |d: Duration| Instant::now() + d;

        store.refresh();
        assert_eq!(sent(&mut requests), ["refresh"]);
        store.refresh();
        store.poll_at(later(PULL_EVERY * 2));
        assert!(
            sent(&mut requests).is_empty(),
            "no second pull, manual or timed, while one is running"
        );
        deliver(
            &events,
            Event::Pulled {
                patients: Vec::new(),
                records: Vec::new(),
                games: Vec::new(),
            },
        );
        store.poll();
        assert!(!store.status().loading);
        store.poll_at(later(Duration::from_secs(4 * 60)));
        assert!(
            sent(&mut requests).is_empty(),
            "the next pull waits 5 minutes"
        );

        store.add_record(record("r1", 2, Some(Utc::now())));
        assert_eq!(sent(&mut requests), ["r1"], "new writes go out at once");
        deliver(
            &events,
            Event::WriteFailed {
                collection: firestore::RECORDS.into(),
                id: "r1".into(),
                seq: seq_of(&store, "r1"),
                message: "Cannot reach Firestore (timeout) - will retry".into(),
            },
        );
        store.poll();
        assert!(sent(&mut requests).is_empty(), "no instant retry loop");
        store.poll_at(later(RETRY_EVERY + Duration::from_secs(1)));
        assert_eq!(
            sent(&mut requests),
            ["r1"],
            "failed writes are retried after 30 s, without a pull"
        );
        store.poll_at(later(PULL_EVERY + Duration::from_secs(1)));
        assert_eq!(
            sent(&mut requests),
            ["refresh"],
            "a pull every 5 minutes, without duplicating the in-flight write"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_sync_target_keeps_its_own_cache_file() {
        assert_eq!(
            cache_file_name(Some(firestore::DEFAULT_PROJECT_ID)),
            "desktop-cache.json",
            "the shared project keeps the existing cache"
        );
        assert_eq!(cache_file_name(None), "desktop-cache-offline.json");
        assert_eq!(
            cache_file_name(Some("kinetism-test-1")),
            "desktop-cache-kinetism-test-1.json"
        );
        assert_eq!(
            cache_file_name(Some(" ../Ward 3\\x ")),
            "desktop-cache-___ward_3_x.json",
            "no folders or odd characters in the file name"
        );
        for project in ["offline", "OFFLINE"] {
            assert_ne!(
                cache_file_name(Some(project)),
                cache_file_name(None),
                "a project called {project} does not share the local-only cache"
            );
        }

        let runtime = runtime();
        let dir = test_dir("targets");
        let at = |project: Option<&str>| {
            DataStore::open(
                runtime.handle(),
                eframe::egui::Context::default(),
                None,
                dir.join(cache_file_name(project)),
            )
        };
        at(None).add_record(record("r-offline", 2, None));
        let shared = at(Some(firestore::DEFAULT_PROJECT_ID));
        assert!(
            shared.records.is_empty() && shared.pending_count() == 0,
            "offline records are never queued for the shared project"
        );
        assert_eq!(
            at(None).pending_count(),
            1,
            "they stay in the offline cache"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_record_is_only_found_for_its_own_patient() {
        let runtime = runtime();
        let dir = test_dir("owner");
        let mut store = open(&dir, &runtime);
        store.add_record(record("r1", 3, None));
        assert!(store.patient_record("p1", "r1").is_some());
        assert!(
            store.patient_record("p2", "r1").is_none(),
            "another patient's screen cannot show or re-grade it"
        );
        assert!(store.patient_record("p1", "r9").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn absurd_pulled_numbers_cannot_make_the_cache_unreadable() {
        let doc = |v: serde_json::Value| v.as_object().cloned().expect("a document");
        let record = firestore::record_from_fields(
            "r-bad",
            &doc(serde_json::json!({
                "patientId": {"stringValue": "p1"},
                "bodyPart": {"stringValue": "Shoulder"},
                "bicepCircumferenceCm": {"doubleValue": 1e155},
                "forearmCircumferenceCm": {"doubleValue": "1e400"},
                "armLengthCm": {"doubleValue": -54.0},
                "angleDegrees": {"doubleValue": 1e300},
                "peakAngleDegrees": {"doubleValue": "inf"},
                "torqueNm": {"doubleValue": "-1e400"},
                "planeDegrees": {"doubleValue": "NaN"}
            })),
        );
        let numbers = [
            record.arm.bicep_cm,
            record.arm.forearm_cm,
            record.arm.arm_length_cm,
            record.angle_degrees,
            record.force_newtons,
        ];
        assert!(numbers.iter().all(|v| v.is_finite()), "{numbers:?}");
        assert_eq!(
            record.arm,
            Anthropometrics {
                bicep_cm: 28.0,
                forearm_cm: 24.0,
                arm_length_cm: 54.0
            },
            "impossible arm dimensions fall back to the phone app's defaults"
        );
        assert_eq!(
            (
                record.peak_angle_degrees,
                record.torque_nm,
                record.plane_degrees
            ),
            (None, None, None),
            "non-finite values are dropped"
        );
        let patient = firestore::patient_from_fields(
            "p1",
            &doc(serde_json::json!({
                "name": {"stringValue": "Siti"},
                "bicepCircumferenceCm": {"doubleValue": 1e155},
                "forearmCircumferenceCm": {"doubleValue": 24.0},
                "armLengthCm": {"doubleValue": 54.0}
            })),
        );
        assert!(
            patient.arm.is_none(),
            "a patient's impossible arm is ignored"
        );

        let cache = CacheFile {
            patients: vec![patient.clone()],
            records: vec![record.clone()],
            ..CacheFile::default()
        };
        let text = serde_json::to_string(&cache).expect("the cache serialises");
        let back: CacheFile = serde_json::from_str(&text).expect("and reads back");
        assert_eq!((back.patients, back.records), (vec![patient], vec![record]));
    }

    #[test]
    fn unreadable_or_unwritable_cache_is_reported() {
        let runtime = runtime();
        let dir = test_dir("corrupt");
        std::fs::create_dir_all(&dir).expect("scratch folder");
        std::fs::write(dir.join("desktop-cache.json"), "{ not json").expect("cache file");
        let mut store = open(&dir, &runtime);
        assert!(
            store
                .cache_error()
                .is_some_and(|e| e.starts_with("Local cache unreadable")),
            "{:?}",
            store.cache_error()
        );
        assert!(store.patients.is_empty() && store.pending_count() == 0);
        let kept = backups(&dir);
        assert_eq!(kept.len(), 1, "the corrupt cache is moved aside: {kept:?}");
        assert_eq!(
            std::fs::read_to_string(&kept[0]).expect("backup"),
            "{ not json"
        );
        store.add_record(record("r0", 3, None));
        assert!(
            store
                .cache_error()
                .is_some_and(|e| e.starts_with("Local cache unreadable")),
            "the warning stays after a successful save: {:?}",
            store.cache_error()
        );
        assert_eq!(
            std::fs::read_to_string(&kept[0]).expect("backup"),
            "{ not json",
            "saving never touches the backup"
        );
        let reopened = open(&dir, &runtime);
        assert!(
            reopened.cache_error().is_none(),
            "{:?}",
            reopened.cache_error()
        );
        assert_eq!(
            reopened.pending_count(),
            1,
            "the new cache holds the queued record"
        );

        let blocker = dir.join("not-a-folder");
        std::fs::write(&blocker, "x").expect("blocking file");
        let mut store = open(&blocker, &runtime);
        assert!(store.cache_error().is_none());
        store.add_record(record("r1", 2, None));
        assert!(
            store
                .cache_error()
                .is_some_and(|e| e.starts_with("Could not save the local cache")),
            "{:?}",
            store.cache_error()
        );
        assert_eq!(store.pending_count(), 1, "the write stays queued in memory");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Files the store moved aside because they could not be read.
    fn backups(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.starts_with("desktop-cache.unreadable-"))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn a_cache_that_is_not_utf8_is_kept_not_overwritten() {
        let runtime = runtime();
        let dir = test_dir("utf16");
        std::fs::create_dir_all(&dir).expect("scratch folder");
        // Notepad's "Unicode" encoding: UTF-16 LE with a byte-order mark.
        let utf16: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain(
                "{\"patients\":[]}"
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes),
            )
            .collect();
        std::fs::write(dir.join("desktop-cache.json"), &utf16).expect("cache file");
        let mut store = open(&dir, &runtime);
        assert!(
            store
                .cache_error()
                .is_some_and(|e| e.starts_with("Local cache unreadable")),
            "an unreadable cache is reported, not silently dropped: {:?}",
            store.cache_error()
        );
        store.add_record(record("r1", 2, None));
        let kept = backups(&dir);
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert_eq!(
            std::fs::read(&kept[0]).expect("backup"),
            utf16,
            "the original bytes survive the next save"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_locked_cache_is_never_overwritten() {
        use std::os::windows::fs::OpenOptionsExt;
        let runtime = runtime();
        let dir = test_dir("locked");
        std::fs::create_dir_all(&dir).expect("scratch folder");
        let path = dir.join("desktop-cache.json");
        let original = "{\"records\":[]}";
        std::fs::write(&path, original).expect("cache file");
        // Another program holds the file without sharing: reading and moving it fail.
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .expect("exclusive handle");
        let mut store = open(&dir, &runtime);
        assert!(
            store
                .cache_error()
                .is_some_and(|e| e.contains("Local saving is paused")),
            "{:?}",
            store.cache_error()
        );
        store.add_record(record("r1", 2, None));
        drop(lock);
        store.add_record(record("r2", 3, None));
        assert_eq!(
            store.pending_count(),
            2,
            "records stay queued for Firestore"
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("cache"),
            original,
            "the cache is not overwritten even after the lock is released"
        );
        assert!(backups(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
