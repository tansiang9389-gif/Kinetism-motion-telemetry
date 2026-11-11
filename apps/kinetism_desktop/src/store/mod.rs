//! Offline-first data store: local JSON cache + Firestore sync worker.
//!
//! Every change is applied locally and cached immediately, then queued for
//! Firestore. Pending writes survive restarts and are retried until the
//! backend accepts them. Remote data is pulled on start and every 30 s so
//! records captured on the phone app show up here.

pub mod firestore;

use crate::clinical::Side;
use crate::model::{GameSession, MeasurementRecord, Patient};
use chrono::{DateTime, Utc};
use firestore::{Fields, FirestoreClient};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

const REFRESH_EVERY: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PendingWrite {
    pub collection: String,
    pub id: String,
    pub fields: Fields,
    pub merge: bool,
    #[serde(skip)]
    pub in_flight: bool,
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
    },
    WriteFailed {
        collection: String,
        id: String,
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
    status: SyncStatus,
    cache_path: PathBuf,
    requests: Option<UnboundedSender<Request>>,
    events: Option<UnboundedReceiver<Event>>,
    ctx: eframe::egui::Context,
    last_refresh: Instant,
    cache_error: Option<String>,
}

pub fn data_dir() -> PathBuf {
    std::env::var_os("KINETISM_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| d.join("Kinetism")))
        .unwrap_or_else(|| PathBuf::from("."))
}

impl DataStore {
    /// `project_id = None` keeps everything local (used for demos and tests).
    pub fn open(
        runtime: &tokio::runtime::Handle,
        ctx: eframe::egui::Context,
        project_id: Option<String>,
        dir: PathBuf,
    ) -> Self {
        let cache_path = dir.join("desktop-cache.json");
        let (cache, cache_error) = match std::fs::read_to_string(&cache_path) {
            Ok(text) => match serde_json::from_str::<CacheFile>(&text) {
                Ok(c) => (c, None),
                Err(e) => (
                    CacheFile::default(),
                    Some(format!(
                        "Local cache unreadable ({e}); starting from Firestore data."
                    )),
                ),
            },
            Err(_) => (CacheFile::default(), None),
        };
        let mut store = Self {
            patients: cache.patients,
            records: cache.records,
            games: cache.games,
            settings: cache.settings,
            pending: cache.pending,
            status: SyncStatus {
                enabled: project_id.is_some(),
                ..SyncStatus::default()
            },
            cache_path,
            requests: None,
            events: None,
            ctx,
            last_refresh: Instant::now(),
            cache_error,
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
        self.cache_error.as_deref()
    }

    pub fn refresh(&mut self) {
        if let Some(tx) = &self.requests {
            self.status.loading = true;
            let _ = tx.send(Request::Refresh);
        }
        self.last_refresh = Instant::now();
    }

    fn flush_pending(&mut self) {
        let Some(tx) = &self.requests else {
            return;
        };
        for write in self.pending.iter_mut().filter(|w| !w.in_flight) {
            write.in_flight = true;
            let _ = tx.send(Request::Write(write.clone()));
        }
    }

    /// Process worker events; call once per frame.
    pub fn poll(&mut self) {
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
                Event::Written { collection, id } => {
                    self.pending
                        .retain(|w| !(w.collection == collection && w.id == id));
                    if self.pending.is_empty() {
                        self.status.last_error = None;
                    }
                    changed = true;
                }
                Event::WriteFailed {
                    collection,
                    id,
                    message,
                } => {
                    if let Some(w) = self
                        .pending
                        .iter_mut()
                        .find(|w| w.collection == collection && w.id == id)
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
        if self.status.enabled && self.last_refresh.elapsed() > REFRESH_EVERY {
            self.refresh();
            self.flush_pending();
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
        self.pending.push(PendingWrite {
            collection: collection.into(),
            id: id.into(),
            fields,
            merge,
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
                },
                Err(e) => Event::WriteFailed {
                    collection: w.collection,
                    id: w.id,
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
            dir.to_path_buf(),
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
                message: offline.into(),
            },
        );
        deliver(
            &events,
            Event::Written {
                collection: firestore::RECORDS.into(),
                id: "r2".into(),
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

        store.last_refresh = Instant::now() - REFRESH_EVERY - Duration::from_secs(1);
        store.poll();
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

    #[test]
    fn unreadable_or_unwritable_cache_is_reported() {
        let runtime = runtime();
        let dir = test_dir("corrupt");
        std::fs::create_dir_all(&dir).expect("scratch folder");
        std::fs::write(dir.join("desktop-cache.json"), "{ not json").expect("cache file");
        let store = open(&dir, &runtime);
        assert!(
            store
                .cache_error()
                .is_some_and(|e| e.starts_with("Local cache unreadable")),
            "{:?}",
            store.cache_error()
        );
        assert!(store.patients.is_empty() && store.pending_count() == 0);

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
}
