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
}
