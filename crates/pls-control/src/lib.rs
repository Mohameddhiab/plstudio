//! Serveur du canal de contrôle de plstudio (`plstudio-control/1`).
//!
//! JSON-lines sur un transport duplex : la session authentifie d'abord le
//! client (`hello` + token de session), puis expose le registre de
//! commandes du moteur (`engine.execute`, `engine.cancel`, `engine.jobs`,
//! `commands.list`). Le transport TCP local (token obligatoire) est livré
//! ici ; les pipes nommées/UDS prendront la même place via le trait
//! [`LineTransport`], sans toucher à la logique de session.
//!
//! L'état de la session ([`ControlSession`]) est scindé du serveur
//! ([`ControlServer`]) : un serveur sert des connexions indépendantes, qui
//! doivent chacune réussir le handshake, et plusieurs connexions peuvent
//! coexister sur la même instance.
//!
//! Voir `docs/control-protocol.md` pour le contrat complet et l'écart
//! entre codes enveloppe (`PLS-E-4xxx`) et codes commande (`PLS-E-1xxx`).

#![forbid(unsafe_code)]

mod proto;
mod server;
mod tcp;
mod token;

pub use proto::{IncomingRequest, JsonResponse, MAX_LINE_BYTES, parse_request};
pub use server::{ControlServer, ControlSession, SessionPolicy};
pub use tcp::{LineTransport, TcpLineTransport, run_until_signal, serve_session};
pub use token::ControlToken;

/// Version de protocole parlée par ce serveur.
pub const PROTOCOL: &str = "plstudio-control/1";
