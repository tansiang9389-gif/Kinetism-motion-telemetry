//! Firestore REST codec and client.
//!
//! Uses the same project and collections as the phone app (`Patients`,
//! `MeasurementRecords`) plus `GameSessions`. Field names match the Flutter
//! models so records written here appear in the phone app's record log.

use crate::clinical::{
    self, Anthropometrics, Movement, PHONE_APP_DEFAULT_ACCEL_G, Side, grade_elbow_flexion,
    grade_shoulder_abduction,
};
use crate::model::{GameLevel, GameSession, MeasurementRecord, MmtSource, Patient};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value, json};
use std::time::Duration;

pub type Fields = Map<String, Value>;

pub const PATIENTS: &str = "Patients";
pub const RECORDS: &str = "MeasurementRecords";
pub const GAMES: &str = "GameSessions";
pub const DEFAULT_PROJECT_ID: &str = "armtracker-84a82";

// ---------------------------------------------------------------- encoding

pub fn string(v: &str) -> Value {
    json!({ "stringValue": v })
}

pub fn double(v: f64) -> Value {
    json!({ "doubleValue": v })
}

pub fn integer(v: i64) -> Value {
    json!({ "integerValue": v.to_string() })
}

pub fn timestamp(v: DateTime<Utc>) -> Value {
    json!({ "timestampValue": v.to_rfc3339_opts(SecondsFormat::Millis, true) })
}

pub fn array(values: Vec<Value>) -> Value {
    json!({ "arrayValue": { "values": values } })
}

pub fn get_str(fields: &Fields, key: &str) -> Option<String> {
    fields
        .get(key)?
        .get("stringValue")?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn get_f64(fields: &Fields, key: &str) -> Option<f64> {
    let v = fields.get(key)?;
    let n = if let Some(d) = v.get("doubleValue") {
        d.as_f64()
            .or_else(|| d.as_str().and_then(|s| s.parse().ok()))
    } else if let Some(i) = v.get("integerValue") {
        i.as_str()
            .and_then(|s| s.parse::<i64>().ok())
            .or_else(|| i.as_i64())
            .map(|i| i as f64)
    } else {
        None
    }?;
    n.is_finite().then_some(n)
}

pub fn get_i64(fields: &Fields, key: &str) -> Option<i64> {
    get_f64(fields, key).map(|v| v.round() as i64)
}

pub fn get_time(fields: &Fields, key: &str) -> Option<DateTime<Utc>> {
    let raw = fields.get(key)?.get("timestampValue")?.as_str()?;
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

// ---------------------------------------------------------------- patients

pub fn patient_to_fields(p: &Patient) -> Fields {
    let mut f = Fields::new();
    f.insert("name".into(), string(&p.name));
    f.insert("code".into(), string(&p.code));
    if let Some(arm) = p.arm {
        f.insert("bicepCircumferenceCm".into(), double(arm.bicep_cm));
        f.insert("forearmCircumferenceCm".into(), double(arm.forearm_cm));
        f.insert("armLengthCm".into(), double(arm.arm_length_cm));
    }
    if let Some(side) = p.affected_side {
        f.insert("affectedSide".into(), string(side.key()));
    }
    f.insert(
        "updatedAt".into(),
        timestamp(p.updated_at.unwrap_or_else(Utc::now)),
    );
    f
}

pub fn patient_from_fields(id: &str, f: &Fields) -> Patient {
    let arm = match (
        get_f64(f, "bicepCircumferenceCm"),
        get_f64(f, "forearmCircumferenceCm"),
        get_f64(f, "armLengthCm"),
    ) {
        (Some(b), Some(fa), Some(l)) if b > 0.0 && fa > 0.0 && l > 0.0 => Some(Anthropometrics {
            bicep_cm: b,
            forearm_cm: fa,
            arm_length_cm: l,
        }),
        _ => None,
    };
    Patient {
        id: id.to_string(),
        name: get_str(f, "name").unwrap_or_else(|| "Unnamed patient".into()),
        code: get_str(f, "code").unwrap_or_else(|| id.to_uppercase()),
        arm,
        affected_side: get_str(f, "affectedSide").and_then(|s| Side::from_key(&s)),
        updated_at: get_time(f, "updatedAt"),
    }
}

// ---------------------------------------------------------------- records

pub fn record_to_fields(r: &MeasurementRecord) -> Fields {
    let mut f = Fields::new();
    f.insert("patientId".into(), string(&r.patient_id));
    f.insert("patientName".into(), string(&r.patient_name));
    f.insert("bodyPart".into(), string(&r.body_part));
    f.insert("movement".into(), string(r.movement.key()));
    if let Some(side) = r.side {
        f.insert("side".into(), string(side.key()));
    }
    f.insert("bicepCircumferenceCm".into(), double(r.arm.bicep_cm));
    f.insert("forearmCircumferenceCm".into(), double(r.arm.forearm_cm));
    f.insert("armLengthCm".into(), double(r.arm.arm_length_cm));
    f.insert("angleDegrees".into(), double(r.angle_degrees));
    if let Some(v) = r.peak_angle_degrees {
        f.insert("peakAngleDegrees".into(), double(v));
    }
    f.insert("forceNewtons".into(), double(r.force_newtons));
    if let Some(v) = r.torque_nm {
        f.insert("torqueNm".into(), double(v));
    }
    f.insert("mmtGrade".into(), integer(i64::from(r.mmt_grade)));
    f.insert("mmtTitle".into(), string(&r.mmt_title));
    f.insert("mmtRationale".into(), string(&r.mmt_rationale));
    f.insert("mmtSource".into(), string(r.mmt_source.key()));
    if let Some(g) = r.auto_mmt_grade {
        f.insert("autoMmtGrade".into(), integer(i64::from(g)));
    }
    f.insert("attemptNumber".into(), integer(i64::from(r.attempt_number)));
    if let Some(v) = r.elbow_flexion_degrees {
        f.insert("elbowFlexionDegrees".into(), double(v));
    }
    if let Some(v) = r.plane_degrees {
        f.insert("planeDegrees".into(), double(v));
    }
    f.insert(
        "muscles".into(),
        array(r.muscles().iter().map(|m| string(m.key())).collect()),
    );
    f.insert("source".into(), string(&r.source));
    f.insert(
        "date".into(),
        timestamp(r.recorded_at.unwrap_or_else(Utc::now)),
    );
    f
}

/// Mirrors the phone app's defensive `MeasurementRecord.fromMap` defaults.
pub fn record_from_fields(id: &str, f: &Fields) -> MeasurementRecord {
    let patient_id = get_str(f, "patientId").unwrap_or_else(|| "unknown-patient".into());
    let body_part = get_str(f, "bodyPart").unwrap_or_else(|| "Arm".into());
    let movement = Movement::infer(get_str(f, "movement").as_deref(), &body_part);
    let arm = Anthropometrics {
        bicep_cm: get_f64(f, "bicepCircumferenceCm").unwrap_or(28.0),
        forearm_cm: get_f64(f, "forearmCircumferenceCm").unwrap_or(24.0),
        arm_length_cm: get_f64(f, "armLengthCm").unwrap_or(54.0),
    };
    let angle = get_f64(f, "angleDegrees").unwrap_or(90.0);
    let force = get_f64(f, "forceNewtons").unwrap_or_else(|| match movement {
        Movement::ElbowFlexion => arm.elbow_force_newtons(angle, PHONE_APP_DEFAULT_ACCEL_G),
        Movement::ShoulderAbduction => arm.shoulder_load_newtons(angle),
    });
    let fallback = match movement {
        Movement::ElbowFlexion => grade_elbow_flexion(angle, force, &arm),
        Movement::ShoulderAbduction => grade_shoulder_abduction(angle, None, None),
    };
    let grade = get_i64(f, "mmtGrade")
        .map(|g| g.clamp(0, 5) as u8)
        .unwrap_or(fallback.grade);
    MeasurementRecord {
        id: id.to_string(),
        patient_name: get_str(f, "patientName").unwrap_or_else(|| patient_id.clone()),
        patient_id,
        body_part,
        movement,
        side: get_str(f, "side").and_then(|s| Side::from_key(&s)),
        arm,
        angle_degrees: angle,
        peak_angle_degrees: get_f64(f, "peakAngleDegrees"),
        force_newtons: force,
        torque_nm: get_f64(f, "torqueNm"),
        mmt_grade: grade,
        mmt_title: get_str(f, "mmtTitle").unwrap_or_else(|| clinical::mmt_title(grade).into()),
        mmt_rationale: get_str(f, "mmtRationale").unwrap_or(fallback.rationale),
        mmt_source: match get_str(f, "mmtSource").as_deref() {
            Some("physio") => MmtSource::Physio,
            _ => MmtSource::Auto,
        },
        auto_mmt_grade: get_i64(f, "autoMmtGrade").map(|g| g.clamp(0, 5) as u8),
        attempt_number: get_i64(f, "attemptNumber")
            .filter(|n| *n >= 1)
            .map(|n| n as u32)
            .unwrap_or(1),
        elbow_flexion_degrees: get_f64(f, "elbowFlexionDegrees"),
        plane_degrees: get_f64(f, "planeDegrees"),
        recorded_at: get_time(f, "date"),
        source: get_str(f, "source").unwrap_or_else(|| "phone".into()),
    }
}

// ---------------------------------------------------------------- games

pub fn game_to_fields(g: &GameSession) -> Fields {
    let mut f = Fields::new();
    f.insert("patientId".into(), string(&g.patient_id));
    f.insert("patientName".into(), string(&g.patient_name));
    f.insert("side".into(), string(g.side.key()));
    f.insert("level".into(), integer(i64::from(g.level.index())));
    f.insert("levelKey".into(), string(g.level.key()));
    f.insert("scorePercent".into(), double(g.score_percent));
    f.insert("points".into(), integer(g.points));
    f.insert("stars".into(), integer(i64::from(g.stars)));
    f.insert("reps".into(), integer(i64::from(g.reps)));
    f.insert("cleanReps".into(), integer(i64::from(g.clean_reps)));
    f.insert("bombsHit".into(), integer(i64::from(g.bombs_hit)));
    f.insert("maxCombo".into(), integer(i64::from(g.max_combo)));
    f.insert("accuracyPercent".into(), double(g.accuracy_percent));
    f.insert("completionPercent".into(), double(g.completion_percent));
    f.insert("durationSeconds".into(), double(g.duration_seconds));
    f.insert("maxElevationDegrees".into(), double(g.max_elevation_deg));
    f.insert(
        "muscles".into(),
        array(g.level.muscles().iter().map(|m| string(m.key())).collect()),
    );
    f.insert("source".into(), string(&g.source));
    f.insert(
        "date".into(),
        timestamp(g.recorded_at.unwrap_or_else(Utc::now)),
    );
    f
}

pub fn game_from_fields(id: &str, f: &Fields) -> Option<GameSession> {
    let level = get_str(f, "levelKey")
        .and_then(|k| GameLevel::ALL.into_iter().find(|l| l.key() == k))
        .or_else(|| get_i64(f, "level").and_then(GameLevel::from_index))?;
    let count = |key: &str| get_i64(f, key).unwrap_or(0).max(0) as u32;
    Some(GameSession {
        id: id.to_string(),
        patient_id: get_str(f, "patientId")?,
        patient_name: get_str(f, "patientName").unwrap_or_default(),
        side: get_str(f, "side")
            .and_then(|s| Side::from_key(&s))
            .unwrap_or_default(),
        level,
        score_percent: get_f64(f, "scorePercent").unwrap_or(0.0).clamp(0.0, 100.0),
        points: get_i64(f, "points").unwrap_or(0),
        stars: get_i64(f, "stars").unwrap_or(0).clamp(0, 3) as u8,
        reps: count("reps"),
        clean_reps: count("cleanReps"),
        bombs_hit: count("bombsHit"),
        max_combo: count("maxCombo"),
        accuracy_percent: get_f64(f, "accuracyPercent").unwrap_or(0.0),
        completion_percent: get_f64(f, "completionPercent").unwrap_or(0.0),
        duration_seconds: get_f64(f, "durationSeconds").unwrap_or(0.0),
        max_elevation_deg: get_f64(f, "maxElevationDegrees").unwrap_or(0.0),
        recorded_at: get_time(f, "date"),
        source: get_str(f, "source").unwrap_or_default(),
    })
}

// ---------------------------------------------------------------- client

#[derive(Clone)]
pub struct FirestoreClient {
    http: reqwest::Client,
    base: String,
}

#[derive(Debug)]
pub struct FsError {
    pub message: String,
    /// Network-level failure (retry later) rather than a rejected request.
    pub offline: bool,
}

impl std::fmt::Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl FirestoreClient {
    pub fn new(project_id: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Self {
            http,
            base: format!(
                "https://firestore.googleapis.com/v1/projects/{project_id}/databases/(default)/documents"
            ),
        }
    }

    async fn check(response: reqwest::Response) -> Result<Value, FsError> {
        let status = response.status();
        let body: Value = response.json().await.unwrap_or(Value::Null);
        if status.is_success() {
            return Ok(body);
        }
        let detail = body
            .pointer("/error/status")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let message = body
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("request rejected");
        Err(FsError {
            message: format!("Firestore {} {detail}: {message}", status.as_u16()),
            offline: status.is_server_error() || status.as_u16() == 429,
        })
    }

    fn network(e: reqwest::Error) -> FsError {
        FsError {
            message: format!("Cannot reach Firestore ({e})"),
            offline: true,
        }
    }

    pub async fn list(&self, collection: &str) -> Result<Vec<(String, Fields)>, FsError> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut request = self
                .http
                .get(format!("{}/{collection}", self.base))
                .query(&[("pageSize", "300")]);
            if let Some(t) = &token {
                request = request.query(&[("pageToken", t.as_str())]);
            }
            let body = Self::check(request.send().await.map_err(Self::network)?).await?;
            if let Some(docs) = body.get("documents").and_then(Value::as_array) {
                for doc in docs {
                    let Some(name) = doc.get("name").and_then(Value::as_str) else {
                        continue;
                    };
                    let id = name.rsplit('/').next().unwrap_or(name).to_string();
                    let fields = doc
                        .get("fields")
                        .and_then(Value::as_object)
                        .cloned()
                        .unwrap_or_default();
                    out.push((id, fields));
                }
            }
            token = body
                .get("nextPageToken")
                .and_then(Value::as_str)
                .map(str::to_string);
            if token.is_none() {
                return Ok(out);
            }
        }
    }

    /// Create or update a document. `merge` keeps fields that are not sent
    /// (like the phone app's `SetOptions(merge: true)`).
    pub async fn upsert(
        &self,
        collection: &str,
        id: &str,
        fields: &Fields,
        merge: bool,
    ) -> Result<(), FsError> {
        let mut request = self
            .http
            .patch(format!("{}/{collection}/{id}", self.base))
            .json(&json!({ "fields": fields }));
        if merge {
            let mask: Vec<(&str, &str)> = fields
                .keys()
                .map(|k| ("updateMask.fieldPaths", k.as_str()))
                .collect();
            request = request.query(&mask);
        }
        Self::check(request.send().await.map_err(Self::network)?)
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(v: Value) -> Fields {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn decodes_phone_app_record_without_new_fields() {
        let f = doc(json!({
            "patientId": {"stringValue": "patient-ahmad"},
            "patientName": {"stringValue": "Ahmad Razak"},
            "bodyPart": {"stringValue": "Arm"},
            "bicepCircumferenceCm": {"doubleValue": 30.5},
            "forearmCircumferenceCm": {"doubleValue": 25},
            "armLengthCm": {"integerValue": "56"},
            "angleDegrees": {"doubleValue": 122.0},
            "forceNewtons": {"doubleValue": 31.2},
            "mmtGrade": {"integerValue": "4"},
            "mmtTitle": {"stringValue": "Good"},
            "mmtRationale": {"stringValue": "Full functional range"},
            "attemptNumber": {"integerValue": "2"},
            "date": {"timestampValue": "2026-11-05T14:29:07.123Z"}
        }));
        let r = record_from_fields("rec-1", &f);
        assert_eq!(r.movement, Movement::ElbowFlexion);
        assert_eq!(r.mmt_grade, 4);
        assert_eq!(r.attempt_number, 2);
        assert_eq!(r.arm.arm_length_cm, 56.0);
        assert_eq!(r.source, "phone");
        assert!(r.recorded_at.is_some());
    }

    #[test]
    fn record_round_trips_through_fields() {
        let r = MeasurementRecord {
            id: "rec-x".into(),
            patient_id: "p1".into(),
            patient_name: "Test".into(),
            body_part: "Shoulder".into(),
            movement: Movement::ShoulderAbduction,
            side: Some(Side::Left),
            arm: Anthropometrics::default(),
            angle_degrees: 72.5,
            peak_angle_degrees: Some(80.0),
            force_newtons: 20.1,
            torque_nm: Some(4.3),
            mmt_grade: 4,
            mmt_title: "Good".into(),
            mmt_rationale: "physio".into(),
            mmt_source: MmtSource::Physio,
            auto_mmt_grade: Some(3),
            attempt_number: 3,
            elbow_flexion_degrees: Some(5.0),
            plane_degrees: Some(-4.0),
            recorded_at: Some(
                DateTime::parse_from_rfc3339("2026-11-11T02:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            source: "kinetism-desktop".into(),
        };
        let back = record_from_fields("rec-x", &record_to_fields(&r));
        assert_eq!(back, r);
        let muscles = record_to_fields(&r)["muscles"].clone();
        assert_eq!(
            muscles["arrayValue"]["values"][0]["stringValue"],
            "middle_deltoid"
        );
    }

    #[test]
    fn patient_and_game_round_trip() {
        let p = Patient {
            id: "patient-1".into(),
            name: "Siti".into(),
            code: "PT-102".into(),
            arm: Some(Anthropometrics::default()),
            affected_side: Some(Side::Right),
            updated_at: Some(
                DateTime::parse_from_rfc3339("2026-11-10T08:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
        };
        assert_eq!(patient_from_fields("patient-1", &patient_to_fields(&p)), p);

        let g = GameSession {
            id: "game-1".into(),
            patient_id: "patient-1".into(),
            patient_name: "Siti".into(),
            side: Side::Right,
            level: GameLevel::PnfD2,
            score_percent: 84.5,
            points: 420,
            stars: 2,
            reps: 8,
            clean_reps: 6,
            bombs_hit: 1,
            max_combo: 4,
            accuracy_percent: 77.0,
            completion_percent: 100.0,
            duration_seconds: 95.5,
            max_elevation_deg: 131.0,
            recorded_at: p.updated_at,
            source: "kinetism-desktop".into(),
        };
        assert_eq!(game_from_fields("game-1", &game_to_fields(&g)), Some(g));
    }

    #[test]
    fn missing_values_fall_back_safely() {
        let p = patient_from_fields("abc", &Fields::new());
        assert_eq!(p.name, "Unnamed patient");
        assert_eq!(p.code, "ABC");
        assert!(p.arm.is_none());
        assert!(game_from_fields("g", &Fields::new()).is_none());
        let f = doc(json!({"angleDegrees": {"doubleValue": "NaN"}}));
        assert_eq!(get_f64(&f, "angleDegrees"), None);
    }
}
