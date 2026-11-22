// SPDX-License-Identifier: MIT
//! Prompt-free KWin 6 scripting adapter. Replies stay on D-Bus; only our JS
//! source is written to a mode-0700 temporary directory. IDs are never recycled
//! and include KWin's unique bus owner, so a compositor restart invalidates them.
use anyhow::{anyhow, ensure, Context, Result};
use serde::Deserialize;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const PATH: &str = "/org/computer_use/KWinReply";
const IFACE: &str = "org.computer_use.KWinReply";
const CAP: Duration = Duration::from_secs(3);
static SCRIPT_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn occupied_script(xml: &str, id: i32) -> bool {
    let name = format!("Script{id}");
    xml.split("name=").skip(1).any(|attribute| {
        let attribute = attribute.trim_start();
        let Some(quote @ ('\'' | '"')) = attribute.chars().next() else {
            return false;
        };
        attribute[1..].split(quote).next() == Some(name.as_str())
    })
}

#[derive(Clone, Debug, Deserialize)]
pub struct Window {
    pub uuid: String,
    #[serde(default)]
    pub native: bool,
    pub pid: u32,
    pub title: String,
    pub app_name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub client_x: i32,
    pub client_y: i32,
    pub active: bool,
    pub minimized: bool,
    pub on_screen: bool,
    #[serde(default)]
    pub unoccluded: bool,
    pub stacking: i32,
    pub desktops: Vec<String>,
    pub output: String,
    pub scale: f64,
    #[serde(skip)]
    pub id: u64,
}

#[derive(Default)]
struct Ids(Vec<(String, String)>);
impl Ids {
    fn id(&mut self, owner: &str, uuid: &str) -> u64 {
        let key = (owner.to_owned(), uuid.to_owned());
        let index = self
            .0
            .iter()
            .position(|item| item == &key)
            .unwrap_or_else(|| {
                self.0.push(key);
                self.0.len() - 1
            });
        0xD000_0000 + index as u64
    }
}
fn ids() -> &'static Mutex<Ids> {
    static IDS: OnceLock<Mutex<Ids>> = OnceLock::new();
    IDS.get_or_init(Default::default)
}
pub fn knows_window(id: u64) -> bool {
    id.checked_sub(0xD000_0000)
        .is_some_and(|index| index < ids().lock().unwrap().0.len() as u64)
}

struct Reply {
    owner: String,
    tx: tokio::sync::mpsc::Sender<String>,
}
#[zbus::interface(name = "org.computer_use.KWinReply")]
impl Reply {
    async fn report(
        &self,
        raw: String,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<()> {
        if header.sender().map(|s| s.as_str()) != Some(self.owner.as_str()) {
            return Err(zbus::fdo::Error::AccessDenied(
                "Only the session KWin may reply".into(),
            ));
        }
        self.tx
            .try_send(raw)
            .map_err(|_| zbus::fdo::Error::Failed("Reply already received".into()))
    }
}

fn run(
    target: Option<(String, u32)>,
    point: Option<serde_json::Value>,
    deadline: Instant,
) -> Result<Vec<Window>> {
    // A separate runtime also permits synchronous callers inside Tokio tasks.
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
            let _gate = tokio::time::timeout_at(deadline.into(), SCRIPT_GATE.lock()).await
                .context("KWin scripting busy until the operation deadline")?;
            let connection = tokio::time::timeout_at(deadline.into(), zbus::Connection::session()).await??;
            let owner = tokio::time::timeout_at(deadline.into(), async {
                let dbus = zbus::fdo::DBusProxy::new(&connection).await?;
                let owner = dbus.get_name_owner("org.kde.KWin".try_into()?).await?;
                let pid = dbus.get_connection_unix_process_id(owner.clone().into()).await?;
                let uid = dbus.get_connection_unix_user(owner.clone().into()).await?;
                ensure!(uid == super::kwin_helper::current_uid() && super::kwin_helper::is_trusted_kwin_process(pid),
                    "KWin owner is not the trusted session compositor");
                Ok::<_, anyhow::Error>(owner.to_string())
            }).await.context("KWin owner lookup timed out")??;
            if let Some((uuid, _)) = &target {
                ensure!(ids().lock().unwrap().0.iter().any(|key| key == &(owner.clone(), uuid.clone())),
                    "KWin restarted; enumerate windows again");
            }
            let (tx, mut rx) = tokio::sync::mpsc::channel(1);
            connection.object_server().at(PATH, Reply { owner: owner.clone(), tx }).await?;
            let request = serde_json::json!({"target": target.as_ref().map(|t| &t.0), "pid": target.as_ref().map(|t| t.1), "point": point});
            let source = format!("var request = {request};\nfunction report(value) {{ callDBus({}, {}, {}, 'Report', JSON.stringify(value)); }}\ntry {{\n{}\n}} catch (e) {{ report({{error:String(e)}}); }}",
                serde_json::to_string(connection.unique_name().context("No unique bus name")?.as_str())?,
                serde_json::to_string(PATH)?, serde_json::to_string(IFACE)?, include_str!("kwin_script.js"));
            let dir = tempfile::Builder::new().prefix("computer-use-kwin-").tempdir()?;
            let prefix = dir.path().file_name().unwrap().to_string_lossy().into_owned();
            let scripting = zbus::Proxy::new(&connection, owner.as_str(), "/Scripting", "org.kde.kwin.Scripting").await?;
            let introspection = zbus::Proxy::new(&connection, owner.as_str(), "/Scripting", "org.freedesktop.DBus.Introspectable").await?;
            let mut names = Vec::new();
            let result = tokio::time::timeout_at(deadline.into(), async {
                // KWin allocates IDs from scripts.size(), rather than the
                // highest object ID. Holes left by unloaded scripts can make
                // a returned ID point to another script. Reserve inert slots
                // until the returned object path was previously unoccupied.
                // Never run Scripting.start or an occupied ScriptN object.
                for attempt in 0..16 {
                    let before: String = introspection.call("Introspect", &()).await?;
                    let name = format!("{prefix}-{attempt}");
                    let path = dir.path().join(format!("request-{attempt}.js"));
                    std::fs::write(&path, "// Inert reservation; no desktop operations.\n")?;
                    names.push(name.clone());
                    let loaded: i32 = scripting.call("loadScript", &(path.to_str().context("Invalid script path")?, name.as_str())).await?;
                    ensure!(loaded >= 0, "KWin refused loadScript");
                    if occupied_script(&before, loaded) { continue; }
                    std::fs::write(&path, &source)?;
                    let script_path = format!("/Scripting/Script{loaded}");
                    let script = zbus::Proxy::new(&connection, owner.as_str(), script_path.as_str(), "org.kde.kwin.Script").await?;
                    script.call::<_, _, ()>("run", &()).await?;
                    let raw = rx.recv().await.context("KWin callback closed")?;
                    return parse(&raw, &owner);
                }
                anyhow::bail!("KWin scripting IDs remained occupied; no request script was run")
            }).await.context("KWin script reply timed out").and_then(|r| r);
            // Unload by our unique name even if loadScript timed out after KWin
            // registered the script. Never call Scripting.start (runs other scripts).
            let _ = tokio::time::timeout(Duration::from_millis(500), async {
                for name in names.iter().rev() {
                    let _ = scripting.call::<_, _, bool>("unloadScript", &(name.as_str(),)).await;
                }
            }).await;
            result
        })
    }).join().map_err(|_| anyhow!("KWin scripting worker panicked"))?
}

fn parse(raw: &str, owner: &str) -> Result<Vec<Window>> {
    #[derive(Deserialize)]
    struct Snapshot {
        windows: Option<Vec<Window>>,
        error: Option<String>,
    }
    let snapshot: Snapshot = serde_json::from_str(raw)?;
    ensure!(
        snapshot.error.is_none(),
        "{}",
        snapshot.error.unwrap_or_default()
    );
    let mut windows = snapshot.windows.context("KWin omitted windows")?;
    let mut seen = std::collections::HashSet::new();
    ensure!(
        windows.iter().filter(|w| w.active).count() <= 1,
        "Multiple active windows"
    );
    for window in &mut windows {
        ensure!(
            !window.uuid.is_empty() && seen.insert(window.uuid.clone()),
            "Invalid/duplicate KWin UUID"
        );
        ensure!(
            window.pid > 0 && window.scale.is_finite() && window.scale > 0.,
            "Invalid KWin pid/scale"
        );
        window.id = ids().lock().unwrap().id(owner, &window.uuid);
    }
    Ok(windows)
}
pub fn may_be_session() -> bool {
    super::kwin_capture::may_be_session()
}
pub fn list_windows() -> Result<Vec<Window>> {
    ensure!(
        super::is_wayland() && super::kwin_capture::may_be_session(),
        "Not KDE Wayland"
    );
    run(None, None, crate::interaction::operation_deadline(CAP))
}
pub(super) fn list_windows_until(deadline: Instant) -> Result<Vec<Window>> {
    run(None, None, deadline)
}

pub fn window(id: u64, pid: Option<u32>) -> Result<Window> {
    ensure!(knows_window(id), "Unknown KWin script window id");
    list_windows()?
        .into_iter()
        .find(|w| w.id == id && pid.is_none_or(|pid| w.pid == pid))
        .context("KWin window UUID/PID no longer exists")
}
pub fn activate(id: u64, pid: Option<u32>, deadline: Instant) -> Result<()> {
    let (owner, uuid) = ids()
        .lock()
        .unwrap()
        .0
        .get(id.checked_sub(0xD000_0000).context("Invalid KWin id")? as usize)
        .cloned()
        .context("Unknown KWin id")?;
    let _ = owner;
    let pid = pid.context("KWin activation requires the enumerated PID")?;
    let windows = run(Some((uuid, pid)), None, deadline)?;
    ensure!(
        windows
            .iter()
            .any(|w| w.id == id && w.pid == pid && w.active && !w.minimized),
        "foreground_unavailable: KWin did not activate the exact UUID/PID"
    );
    Ok(())
}
pub(super) fn verify_point(id: u64, pid: u32, x: i32, y: i32) -> Result<()> {
    let (_, uuid) = ids()
        .lock()
        .unwrap()
        .0
        .get(id.checked_sub(0xD000_0000).context("Invalid KWin id")? as usize)
        .cloned()
        .context("Unknown KWin id")?;
    let windows = run(
        None,
        Some(serde_json::json!({"uuid":uuid, "pid":pid, "x":x, "y":y})),
        crate::interaction::operation_deadline(CAP),
    )?;
    ensure!(
        windows
            .iter()
            .any(|w| w.id == id && w.pid == pid && w.active),
        "KWin pointer target disappeared"
    );
    Ok(())
}

/// Read-only availability probe. Never creates a portal session or prompts.
pub fn remote_desktop_status() -> Result<String> {
    std::thread::spawn(|| -> Result<String> {
        tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
            tokio::time::timeout(CAP, async {
                let connection = zbus::Connection::session().await?;
                let proxy = zbus::Proxy::new(&connection, "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.RemoteDesktop").await?;
                let version: u32 = proxy.get_property("version").await?;
                ensure!(version >= 2, "RemoteDesktop v{version} lacks ConnectToEIS (requires v2)");
                let devices: u32 = proxy.get_property("AvailableDeviceTypes").await?;
                ensure!(devices & 3 == 3, "RemoteDesktop does not advertise both keyboard and pointer");
                Ok(format!("RemoteDesktop v{version} advertises keyboard and pointer; EIS readiness is checked on delivery"))
            }).await.context("RemoteDesktop availability probe timed out")?
        })
    }).join().map_err(|_| anyhow!("RemoteDesktop probe panicked"))?
}

pub fn infos() -> Result<Vec<crate::x11::WindowInfo>> {
    Ok(list_windows()?
        .into_iter()
        .map(|w| crate::x11::WindowInfo {
            xid: w.id,
            pid: Some(w.pid),
            app_name: w.app_name,
            title: w.title,
            x: w.x,
            y: w.y,
            width: w.width,
            height: w.height,
            is_on_screen: w.on_screen,
            z_index: (w.stacking >= 0).then_some(w.stacking as usize),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn occupied_script_ids_are_not_confused_with_prefixes_or_other_attributes() {
        let xml = "<node><node name=\"Script0\"/><node name='Script10'/></node>";
        assert!(occupied_script(xml, 0));
        assert!(occupied_script(xml, 10));
        assert!(!occupied_script(xml, 1));
        assert!(!occupied_script(xml, 11));
        assert!(!occupied_script("<node name=\"Script1-unrelated\"/>", 1));
    }
    #[test]
    fn ids_are_stable_and_compositor_restarts_invalidate_them() {
        let mut ids = Ids::default();
        assert_eq!(ids.id(":1.2", "a"), ids.id(":1.2", "a"));
        assert_ne!(ids.id(":1.2", "a"), ids.id(":1.3", "a"));
        assert_ne!(ids.id(":1.2", "a"), ids.id(":1.2", "b"));
    }

    fn record() -> serde_json::Value {
        serde_json::json!({"uuid":"{task3-test-window}", "pid":42,
            "title":"Task3 \"quoted\" λ", "app_name":"fixture", "x":-500, "y":20,
            "width":400, "height":300, "client_x":-498, "client_y":48,
            "active":true, "minimized":false, "on_screen":true, "unoccluded":true, "stacking":3,
            "desktops":["desktop-1"], "output":"test-output", "scale":1.5})
    }

    #[test]
    fn snapshot_preserves_unicode_negative_origins_and_fractional_scale() {
        let mut visible = record();
        visible["unoccluded"] = serde_json::json!(true);
        let raw = serde_json::json!({"windows":[visible]}).to_string();
        let windows = parse(&raw, ":task3-test").unwrap();
        assert!(windows[0].unoccluded);
        assert_eq!(windows[0].title, "Task3 \"quoted\" λ");
        assert_eq!(
            (windows[0].x, windows[0].client_y, windows[0].scale),
            (-500, 48, 1.5)
        );
        assert_eq!(windows[0].id, parse(&raw, ":task3-test").unwrap()[0].id);
    }

    #[test]
    fn malformed_or_ambiguous_snapshots_never_authorize_input() {
        for raw in [
            serde_json::json!({"error":"target disappeared"}),
            serde_json::json!({"windows":[record(), record()]}),
            serde_json::json!({"windows":[{"uuid":"missing fields"}]}),
        ] {
            assert!(parse(&raw.to_string(), ":task3-test").is_err());
        }
        for (field, value) in [
            ("pid", serde_json::json!(0)),
            ("scale", serde_json::json!(0)),
            ("uuid", serde_json::json!("")),
            ("width", serde_json::json!(-1)),
        ] {
            let mut window = record();
            window[field] = value;
            assert!(parse(
                &serde_json::json!({"windows":[window]}).to_string(),
                ":task3-test"
            )
            .is_err());
        }
    }
}
