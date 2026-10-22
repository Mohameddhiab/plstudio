//! Fondations partagées de plstudio : erreurs, identifiants, commandes,
//! chemins, settings, événements et exécution de processus.

#![forbid(unsafe_code)]

pub mod command;
pub mod danger;
pub mod error;
pub mod events;
pub mod ids;
pub mod paths;
pub mod process;
pub mod settings;
pub mod trust;

pub use command::{
    Cancellation, Command, CommandCtx, CommandId, CommandMeta, CommandOutput, parse_params,
};
pub use danger::DangerLevel;
pub use error::{ErrorCode, PlsError, Result};
pub use ids::{JobId, RequestId};
pub use paths::PlsPaths;
pub use process::{ProcessOutput, ProcessRunner, ProcessSpec, SystemProcessRunner};
pub use settings::SettingsStore;
pub use trust::TrustLevel;

/// Version du protocole de contrôle exposée par le moteur.
pub const CONTROL_PROTOCOL_VERSION: &str = "plstudio-control/1";
