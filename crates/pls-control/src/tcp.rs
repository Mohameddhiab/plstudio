//! Transport TCP local du canal de contrôle.
//!
//! Pivot documenté : n'écoute que sur `127.0.0.1`, token obligatoire au
//! handshake. Les pipes nommées (Windows) et UDS (Linux) prendront la
//! même place derrière [`LineTransport`] — la session ne connaît rien du
//! transport.

use async_trait::async_trait;
use pls_base::{ErrorCode, PlsError, Result};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream, tcp::OwnedReadHalf},
};

use crate::server::{ControlServer, ControlSession};

/// Un transport duplex du protocole : lignes entrantes, lignes sorties.
#[async_trait]
pub trait LineTransport: Send {
    /// Ligne suivante du client ; `None` quand le canal se ferme.
    async fn next_line(&mut self) -> Result<Option<String>>;

    async fn send_line(&mut self, line: &str) -> Result<()>;
}

/// `LineTransport` sur une connexion TCP locale.
pub struct TcpLineTransport {
    reader: BufReader<OwnedReadHalf>,
    writer: tokio::net::tcp::OwnedWriteHalf,
}

impl TcpLineTransport {
    /// Découpe le stream en demi-tubes et fixe le tampon de lecture.
    pub async fn connect(stream: TcpStream) -> std::io::Result<Self> {
        let (read_half, write_half) = stream.into_split();

        Ok(Self {
            reader: BufReader::with_capacity(4096, read_half),
            writer: write_half,
        })
    }
}

#[async_trait]
impl LineTransport for TcpLineTransport {
    async fn next_line(&mut self) -> Result<Option<String>> {
        let mut buffer = Vec::new();

        loop {
            let available = self.reader.fill_buf().await?;

            if available.is_empty() {
                return Ok(None); // Le client a fermé le canal.
            }

            match available.iter().position(|&byte| byte == b'\n') {
                Some(index) => {
                    buffer.extend_from_slice(&available[..index]);
                    self.reader.consume(index + 1);
                    break;
                }
                None => {
                    let chunk = available.len();
                    buffer.extend_from_slice(available);
                    self.reader.consume(chunk);

                    // Remorquage borné : au-delà de la limite (+ un tampon
                    // de BufReader), la ligne est rejetée avant d'avoir
                    // gonflé la mémoire au gré d'un pair hostile.
                    if buffer.len() > crate::proto::MAX_LINE_BYTES {
                        return Err(PlsError::app(
                            ErrorCode::CONTROL_ENVELOPE,
                            format!(
                                "a request line exceeded {} bytes",
                                crate::proto::MAX_LINE_BYTES
                            ),
                        )
                        .with_remediation("send smaller payloads in one object per line"));
                    }
                }
            }
        }

        let line = String::from_utf8(buffer).map_err(|error| {
            PlsError::app(
                ErrorCode::CONTROL_ENVELOPE,
                format!("a request line was not UTF-8: {error}"),
            )
            .with_remediation("the protocol speaks UTF-8 JSON lines")
        })?;

        Ok(Some(line))
    }

    async fn send_line(&mut self, line: &str) -> Result<()> {
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;

        Ok(())
    }
}

/// Sert une session de contrôle sur le canal, jusqu'à sa fermeture.
///
/// Une ligne vide ne compte pas : les clients qui terminent les lignes
/// par `\r\n` ou qui voient arriver des flush en doublon restent sains.
pub async fn serve_session<T: LineTransport + ?Sized>(
    server: &ControlServer,
    transport: &mut T,
) -> Result<()> {
    let mut session = ControlSession::new(server);
    let mut transport_error: Option<PlsError> = None;

    loop {
        match transport.next_line().await {
            Ok(Some(line)) => {
                let trimmed = line.trim_end_matches(['\n', '\r']);
                if trimmed.is_empty() {
                    continue;
                }

                if let Some(response) = session.dispatch(trimmed).await {
                    transport.send_line(&response.to_line()).await?;
                }
            }
            Ok(None) => break,
            Err(error) => {
                transport_error = Some(error);
                break;
            }
        }
    }

    match transport_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Boucle d'acceptation : une session par connexion, jusqu'à Ctrl+C.
///
/// Les connexions sont indépendantes ; une session qui échoue est loguée
/// puis abandonnée, sans toucher aux autres.
pub async fn run_until_signal(
    listener: TcpListener,
    server: std::sync::Arc<ControlServer>,
) -> Result<()> {
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, peer) = accepted?;

                tracing::debug!(target: "control", "connection from {peer}");
                let server = server.clone();
                tokio::spawn(async move {
                    match TcpLineTransport::connect(stream).await {
                        Ok(mut transport) => {
                            // Une erreur de transport fermera juste cette session.
                            let _ignored = serve_session(&server, &mut transport).await;
                        }
                        Err(error) => {
                            tracing::debug!(target: "control", "split failed: {error}")
                        }
                    }
                });
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!(target: "control", "shutting the control server down");
                break;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PROTOCOL;
    use crate::server::SessionPolicy;
    use crate::token::ControlToken;
    use pls_base::TrustLevel;
    use pls_engine::Engine;
    use serde_json::{Value, json};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::io::BufReader;

    /// Réponse du serveur, lue ligne par ligne en objet analysé.
    type Reply = Value;

    struct TestClient {
        writer: tokio::net::tcp::OwnedWriteHalf,
        reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    }

    impl TestClient {
        fn new(stream: TcpStream) -> Self {
            let (read_half, write_half) = stream.into_split();

            Self {
                writer: write_half,
                reader: BufReader::new(read_half),
            }
        }

        async fn request(&mut self, method: &str, params: Value) -> Reply {
            self.request_raw(format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"{method}\",\"params\":{params}}}\n"
            ))
            .await
        }

        /// Ligne brute (cas d'enveloppe cassée compris).
        async fn request_raw(&mut self, line: impl Into<String>) -> Reply {
            self.writer
                .write_all(line.into().as_bytes())
                .await
                .expect("write reaches the server");
            self.writer.flush().await.expect("flush reaches the server");

            let mut text = String::new();
            self.reader
                .read_line(&mut text)
                .await
                .expect("a reply line");

            serde_json::from_str(text.trim_end()).expect("a reply is valid JSON")
        }
    }

    #[tokio::test]
    async fn a_tcp_client_handshakes_then_executes() {
        let token = ControlToken::generate().expect("randomness available");
        let server = Arc::new(ControlServer::new(
            Arc::new(Engine::new()),
            SessionPolicy {
                workspace: PathBuf::from("."),
                trust: TrustLevel::Trusted,
            },
            token.clone(),
        ));

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();

        // Le test tient le canal d'acceptation : une session, une réalité.
        let serving = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut transport = TcpLineTransport::connect(stream).await.expect("split");
            serve_session(&server, &mut transport)
                .await
                .expect("session serves until the client leaves");
        });

        let mut client = TestClient::new(
            TcpStream::connect(("127.0.0.1", port))
                .await
                .expect("connect"),
        );

        // 1. Avant le handshake : refus net.
        let reply = client.request("engine.jobs", json!({})).await;
        assert_eq!(reply["error"]["code"], json!(4006));

        // 2. Mauvais token au hello : 4001, la porte reste fermée.
        let reply = client
            .request_raw(format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"hello\",\"params\":{{\"protocol\":\"{PROTOCOL}\",\"token\":\"{}\"}}}}\n",
                "f".repeat(64)
            ))
            .await;
        assert_eq!(reply["error"]["data"]["code"], json!("PLS-E-4001"));

        // 3. Le bon token ouvre la session.
        let reply = client
            .request(
                "hello",
                json!({ "protocol": PROTOCOL, "token": token.as_hex() }),
            )
            .await;
        assert_eq!(reply["result"]["protocol"], json!(PROTOCOL));
        assert!(reply["result"]["commands"].as_u64().is_some());

        // 4. Une commande read du registre s'exécute de bout en bout.
        let reply = client
            .request("engine.execute", json!({ "command": "editor.list" }))
            .await;
        assert_eq!(reply["result"]["status"], json!("completed"));

        // 5. Une ligne cassée ne tue pas la session (id null, 4004).
        let reply = client.request_raw("garbage\n").await;
        assert_eq!(reply["error"]["code"], json!(4004));

        // Le client part AVANT d'attendre le serveur : c'est la fermeture
        // du stream qui termine la session côté serveur ; l'inverse est un
        // embouteillage (le drop local arrive après le `.await`).
        drop(client);
        serving
            .await
            .expect("session serves until the client leaves");
    }
}
