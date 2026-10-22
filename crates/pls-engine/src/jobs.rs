//! Suivi des jobs exécutés par le moteur.
//!
//! Un job = une invocation de commande via `engine.execute`. Chaque
//! invocation est enregistrée au démarrage puis passe en état terminal :
//! `succeeded`, `failed`, ou `cancelled` (erreur de code `PLS-E-1007`).
//! La table alimente `engine.jobs` (observabilité) et `engine.cancel`
//! (annulation coopérative via le drapeau de `CommandCtx`).

use std::{
    collections::BTreeMap,
    sync::{Mutex, PoisonError},
    time::{SystemTime, UNIX_EPOCH},
};

use pls_base::{Cancellation, CommandId, DangerLevel, ErrorCode, JobId, PlsError, Result};
use serde::{Deserialize, Serialize};

/// État d'un job dans la table du moteur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    /// En cours d'exécution.
    Running,
    /// Terminé sans erreur.
    Succeeded,
    /// Terminé en erreur.
    Failed,
    /// Annulé : la commande a retourné `PLS-E-1007`.
    Cancelled,
}

impl JobState {
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }
}

/// Erreur d'un job échoué, sérialisable pour la CLI, l'UI et MCP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobError {
    pub code: String,
    pub message: String,
    pub remediation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: JobId,
    pub command: CommandId,
    pub title: String,
    pub danger: DangerLevel,
    pub state: JobState,
    pub started_ms: u64,
    pub finished_ms: Option<u64>,
    pub error: Option<JobError>,
}

impl JobRecord {
    pub fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }
}

#[derive(Debug, Default)]
struct Tables {
    records: BTreeMap<JobId, JobRecord>,
    cancellations: BTreeMap<JobId, Cancellation>,
}

/// Table des jobs, partie du moteur mutable à l'exécution.
#[derive(Debug, Default)]
pub struct JobTable {
    tables: Mutex<Tables>,
}

impl JobTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enregistre un job démarrant et mémorise son drapeau d'annulation.
    pub fn start(
        &self,
        id: JobId,
        command: CommandId,
        title: String,
        danger: DangerLevel,
        cancellation: Cancellation,
    ) {
        let mut tables = self.lock();
        tables.records.insert(
            id,
            JobRecord {
                id,
                command,
                title,
                danger,
                state: JobState::Running,
                started_ms: now_ms(),
                finished_ms: None,
                error: None,
            },
        );
        tables.cancellations.insert(id, cancellation);
    }

    pub fn finish_success(&self, id: JobId) {
        let mut tables = self.lock();
        tables.cancellations.remove(&id);
        if let Some(record) = tables.records.get_mut(&id) {
            record.state = JobState::Succeeded;
            record.finished_ms = Some(now_ms());
        }
    }

    pub fn finish_error(&self, id: JobId, error: &PlsError) {
        let mut tables = self.lock();
        tables.cancellations.remove(&id);
        if let Some(record) = tables.records.get_mut(&id) {
            record.state = if error.code() == ErrorCode::CANCELLED {
                JobState::Cancelled
            } else {
                JobState::Failed
            };
            record.finished_ms = Some(now_ms());
            record.error = Some(job_error(error));
        }
    }

    /// Pose le drapeau d'annulation d'un job en cours.
    ///
    /// L'annulation est coopérative : la commande doit consulter
    /// `ctx.cancellation` pour s'arrêter proprement.
    pub fn cancel(&self, id: JobId) -> Result<()> {
        let tables = self.lock();

        let Some(record) = tables.records.get(&id) else {
            return Err(unknown_job(id));
        };

        if record.is_terminal() {
            return Err(PlsError::app(
                ErrorCode::COMMAND,
                format!("job {id} is already finished (state: {:?})", record.state),
            )
            .with_remediation("list current jobs with `engine.jobs`"));
        }

        if let Some(cancellation) = tables.cancellations.get(&id) {
            cancellation.cancel();
        }

        Ok(())
    }

    pub fn snapshot(&self) -> Vec<JobRecord> {
        self.lock().records.values().cloned().collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Tables> {
        // Un panique dans une section critique ne doit pas contaminer la
        // table : on récupère les données le plus intact possible.
        self.tables.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Plateforme neutre : sérialise une erreur `PlsError` pour la table.
fn job_error(error: &PlsError) -> JobError {
    match error {
        PlsError::App {
            code,
            message,
            remediation,
        }
        | PlsError::Config {
            code,
            message,
            remediation,
        } => JobError {
            code: code.as_str().to_string(),
            message: message.clone(),
            remediation: remediation.clone(),
        },
        PlsError::Io { code, source } => JobError {
            code: code.as_str().to_string(),
            message: format!("I/O error: {source}"),
            remediation: None,
        },
    }
}

fn unknown_job(id: JobId) -> PlsError {
    PlsError::app(ErrorCode::COMMAND, format!("unknown job {id}"))
        .with_remediation("list current jobs with `engine.jobs`")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_rejects_unknown_and_finished_jobs() {
        let table = JobTable::new();
        assert!(table.cancel(JobId::new(42)).is_err());

        table.start(
            JobId::new(1),
            CommandId::from("test.echo"),
            "Echo".to_string(),
            DangerLevel::Read,
            Cancellation::new(),
        );
        table.finish_success(JobId::new(1));

        let error = table.cancel(JobId::new(1)).unwrap_err();
        assert_eq!(error.code(), ErrorCode::COMMAND);
        assert!(error.to_string().contains("already finished"));
    }

    #[test]
    fn finish_error_records_code_message_remediation() {
        let table = JobTable::new();
        table.start(
            JobId::new(1),
            CommandId::from("test.x"),
            "X".to_string(),
            DangerLevel::Read,
            Cancellation::new(),
        );
        table.finish_error(
            JobId::new(1),
            &PlsError::app(ErrorCode::CANCELLED, "stopped").with_remediation("try again"),
        );

        let record = &table.snapshot()[0];
        assert_eq!(record.state, JobState::Cancelled);
        let error = record.error.as_ref().expect("error present");
        assert_eq!(error.code, "PLS-E-1007");
        assert_eq!(error.remediation.as_deref(), Some("try again"));
    }

    #[test]
    fn terminal_states_are_recognized() {
        assert!(JobState::Succeeded.is_terminal());
        assert!(JobState::Failed.is_terminal());
        assert!(JobState::Cancelled.is_terminal());
        assert!(!JobState::Running.is_terminal());
    }
}
