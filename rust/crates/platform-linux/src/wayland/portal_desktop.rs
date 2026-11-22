// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Talos HQ

//! Daemon-owned RemoteDesktop session: one Start for keyboard, pointer and video.
//! A dedicated thread owns the runtime and D-Bus session until process exit.
//! Both success and refusal are cached, including across concurrent tool calls.

use std::os::fd::OwnedFd;
use std::sync::{mpsc, OnceLock};
use std::time::Duration;

use ashpd::desktop::remote_desktop::{
    ConnectToEISOptions, DeviceType, RemoteDesktop, SelectDevicesOptions, StartOptions,
};
use ashpd::desktop::screencast::{
    CursorMode, OpenPipeWireRemoteOptions, Screencast, SelectSourcesOptions, SourceType,
};
use ashpd::desktop::{CreateSessionOptions, PersistMode};
use ashpd::enumflags2::BitFlags;

use super::portal::RestoreToken;

const CALL_TIMEOUT: Duration = Duration::from_secs(10);
const CONSENT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug)]
pub(crate) enum SessionFailure {
    /// Setup failed before Start could display a dialog. Legacy capture may work.
    Unavailable(String),
    /// Start was attempted. Never show a second dialog in this daemon.
    Refused(String),
}

impl std::fmt::Display for SessionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(detail) | Self::Refused(detail) => f.write_str(detail),
        }
    }
}

impl std::error::Error for SessionFailure {}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MonitorStream {
    pub node_id: u32,
    pub position: Option<(i32, i32)>,
    pub size: Option<(i32, i32)>,
}

type Reply<T> = mpsc::SyncSender<Result<T, String>>;

enum Command {
    Eis(Reply<OwnedFd>),
    PipeWire(Reply<(OwnedFd, MonitorStream)>),
}

struct Owner {
    tx: mpsc::Sender<Command>,
}

static OWNER: OnceLock<Result<Owner, SessionFailure>> = OnceLock::new();

pub(crate) fn is_started() -> bool {
    matches!(OWNER.get(), Some(Ok(_)))
}

fn owner() -> Result<&'static Owner, SessionFailure> {
    OWNER
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel();
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            std::thread::Builder::new()
                .name("computer-use-portal".into())
                .spawn(move || {
                    if let Err(error) = run_owner(rx, &ready_tx) {
                        let _ = ready_tx.send(Err(error));
                    }
                })
                .map_err(|error| SessionFailure::Unavailable(error.to_string()))?;
            ready_rx
                .recv_timeout(CALL_TIMEOUT * 2 + CONSENT_TIMEOUT)
                .map_err(|error| SessionFailure::Refused(format!("portal owner: {error}")))??;
            Ok(Owner { tx })
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn request<T>(make: impl FnOnce(Reply<T>) -> Command) -> anyhow::Result<T> {
    let owner = owner()?;
    let (tx, rx) = mpsc::sync_channel(1);
    owner.tx.send(make(tx))?;
    rx.recv_timeout(CALL_TIMEOUT + Duration::from_secs(1))?
        .map_err(anyhow::Error::msg)
}

pub(crate) fn connect_to_eis() -> anyhow::Result<OwnedFd> {
    request(Command::Eis)
}

pub(crate) fn pipewire_stream() -> anyhow::Result<(OwnedFd, MonitorStream)> {
    request(Command::PipeWire)
}

fn run_owner(
    rx: mpsc::Receiver<Command>,
    ready: &mpsc::SyncSender<Result<(), SessionFailure>>,
) -> Result<(), SessionFailure> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .map_err(|error| SessionFailure::Unavailable(error.to_string()))?;
    let (remote, cast, session) = runtime.block_on(async {
        tokio::time::timeout(CALL_TIMEOUT, async {
            let connection = super::portal::fresh_session_connection().await?;
            let remote = RemoteDesktop::with_connection(connection.clone()).await?;
            let cast = Screencast::with_connection(connection).await?;
            let session = remote
                .create_session(CreateSessionOptions::default())
                .await?;
            let token = RestoreToken::Desktop.read();
            let devices = SelectDevicesOptions::default()
                .set_devices(BitFlags::from(DeviceType::Keyboard) | DeviceType::Pointer)
                .set_persist_mode(PersistMode::ExplicitlyRevoked)
                .set_restore_token(token.as_deref());
            remote.select_devices(&session, devices).await?.response()?;
            // ScreenCast accepts RemoteDesktop's session handle. Start is called
            // once, via RemoteDesktop, after both selections are attached.
            let sources = SelectSourcesOptions::default()
                .set_sources(BitFlags::from(SourceType::Monitor))
                .set_multiple(false)
                .set_cursor_mode(CursorMode::Embedded);
            cast.select_sources(&session, sources).await?.response()?;
            anyhow::Ok((remote, cast, session))
        })
        .await
        .map_err(|_| SessionFailure::Unavailable("portal setup timed out".into()))?
        .map_err(|error| SessionFailure::Unavailable(format!("combined portal setup: {error:#}")))
    })?;
    let selected = runtime.block_on(async {
        tokio::time::timeout(CONSENT_TIMEOUT, async {
            remote
                .start(&session, None, StartOptions::default())
                .await?
                .response()
        })
        .await
        .map_err(|_| {
            SessionFailure::Refused(
                "combined portal consent was not granted within 60s; restart the daemon to retry"
                    .into(),
            )
        })?
        .map_err(|error| {
            SessionFailure::Refused(format!(
                "combined portal consent refused: {error}; restart the daemon to retry"
            ))
        })
    });
    let selected = match selected {
        Ok(selected) => selected,
        Err(error) => {
            let _ = runtime.block_on(session.close());
            return Err(error);
        }
    };
    if let Some(token) = selected.restore_token() {
        if let Err(error) = RestoreToken::Desktop.write(token) {
            tracing::warn!("could not persist combined portal restore token: {error:#}");
        }
    }
    let monitor = selected.streams().first().map(|stream| MonitorStream {
        node_id: stream.pipe_wire_node_id(),
        position: stream.position(),
        size: stream.size(),
    });
    tracing::info!(?monitor, devices = ?selected.devices(), "combined portal session started");
    let _ = ready.send(Ok(()));
    // Never drop the runtime/session between tool calls or when a recording ends.
    for command in rx {
        match command {
            Command::Eis(reply) => {
                let result = runtime.block_on(async {
                    tokio::time::timeout(
                        CALL_TIMEOUT,
                        remote.connect_to_eis(&session, ConnectToEISOptions::default()),
                    )
                    .await
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())
                });
                let _ = reply.send(result);
            }
            Command::PipeWire(reply) => {
                let result = runtime.block_on(async {
                    let stream = monitor.ok_or_else(|| {
                        "combined portal session has no monitor stream".to_string()
                    })?;
                    // Each consumer gets its own remote connection to the SAME
                    // node. Duplicating a connected PipeWire socket is unsafe.
                    let fd = tokio::time::timeout(
                        CALL_TIMEOUT,
                        cast.open_pipe_wire_remote(&session, OpenPipeWireRemoteOptions::default()),
                    )
                    .await
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())?;
                    Ok((fd, stream))
                });
                let _ = reply.send(result);
            }
        }
    }
    let _ = runtime.block_on(session.close());
    Ok(())
}

/// Preserve failures once Start has been attempted: falling back after a denied
/// or broken shared stream could display a second, Screenshot-portal dialog.
pub(crate) fn screenshot_route<T>(
    shared: Result<T, anyhow::Error>,
    capture: impl FnOnce(T) -> anyhow::Result<Vec<u8>>,
    fallback: impl FnOnce() -> anyhow::Result<Option<Vec<u8>>>,
) -> anyhow::Result<Option<Vec<u8>>> {
    match shared {
        Ok(stream) => capture(stream).map(Some),
        Err(error)
            if matches!(
                error.downcast_ref::<SessionFailure>(),
                Some(SessionFailure::Unavailable(_))
            ) =>
        {
            fallback()
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshots_use_shared_stream_or_unavailable_portal_fallback() {
        assert_eq!(
            screenshot_route(Ok(7), |node| Ok(vec![node]), || panic!("fallback prompted")).unwrap(),
            Some(vec![7])
        );
        let unavailable = Err::<u8, _>(SessionFailure::Unavailable("unsupported".into()).into());
        assert_eq!(
            screenshot_route(unavailable, |_| panic!("no stream"), || Ok(Some(vec![9]))).unwrap(),
            Some(vec![9])
        );
    }

    #[test]
    fn refused_consent_and_broken_stream_never_prompt_again() {
        let refused = Err::<u8, _>(SessionFailure::Refused("timeout".into()).into());
        assert!(
            screenshot_route(refused, |_| panic!("no stream"), || panic!("second dialog")).is_err()
        );
        assert!(screenshot_route(
            Ok(1),
            |_| anyhow::bail!("lost stream"),
            || panic!("second dialog")
        )
        .is_err());
    }
}
