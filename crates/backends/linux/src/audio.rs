//! Call audio routing through PipeWire.
//!
//! During a hands-free call PipeWire creates two nodes for the phone (see
//! [`pwdump`]). To hear the caller on the PC speakers and talk through the
//! PC microphone, they must be connected to the PC's devices:
//!
//! ```text
//!   phone voice (Audio/Source) ──pw-loopback──▶ PC speakers (Audio/Sink)
//!   PC microphone (Audio/Source) ──pw-loopback──▶ phone (Audio/Sink)
//! ```
//!
//! We use the stock `pw-loopback` tool (one process per direction) rather
//! than a PipeWire library binding: it's what a human would type to test,
//! and [`loopback_command`] is the single place to tweak it. If something
//! (WirePlumber policy, the user in Helvum/qpwgraph) already linked a
//! direction, we leave it alone.
//!
//! Microphone mute = stop the mic loopback; unmute = start it again.

pub mod pwdump;

use async_trait::async_trait;
use phonesym_core::*;
use pwdump::{Graph, Node, PhoneNodes, SINK, SOURCE};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// How long to wait for PipeWire to create the phone's nodes after the call
/// audio link comes up.
const NODE_WAIT: Duration = Duration::from_secs(5);
const NODE_POLL: Duration = Duration::from_millis(300);

/// Which way audio flows through a loopback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    /// Caller's voice -> PC speakers.
    PhoneToSpeakers,
    /// PC microphone -> phone.
    MicToPhone,
}

impl Direction {
    fn node_name(self) -> &'static str {
        match self {
            Self::PhoneToSpeakers => "phonesym-phone-to-speakers",
            Self::MicToPhone => "phonesym-mic-to-phone",
        }
    }
}

/// Build the `pw-loopback` command for one direction.
///
/// - `--channels 1 --channel-map [ MONO ]`: HFP audio is mono; PipeWire
///   up/down-mixes to the stereo PC devices.
/// - `target.object`: connect to exactly this node (by `node.name`) instead
///   of whatever the default is, so we never loop the phone to itself.
/// - `node.dont-reconnect`: if the phone's node disappears (call ended),
///   don't let PipeWire move our stream to another device.
/// - `--latency 40`: small enough for conversation, big enough to avoid
///   crackles on a busy desktop. Tweak here.
///
/// Equivalent shell command, for testing by hand:
/// ```sh
/// pw-loopback -n phonesym-phone-to-speakers -c 1 -m '[ MONO ]' -l 40 \
///   --capture-props='{ target.object="bluez_input.AA_BB_CC_DD_EE_FF.0" node.dont-reconnect=true }' \
///   --playback-props='{ target.object="alsa_output.pci-0000_00_1f.3.analog-stereo" node.dont-reconnect=true }'
/// ```
fn loopback_command(direction: Direction, from: Option<&str>, to: Option<&str>) -> Command {
    let props = |target: Option<&str>| match target {
        Some(name) => format!("{{ target.object=\"{name}\" node.dont-reconnect=true }}"),
        // No local device found: let PipeWire use the default one.
        None => "{ }".to_owned(),
    };
    let mut cmd = Command::new("pw-loopback");
    cmd.arg("--name")
        .arg(direction.node_name())
        .args([
            "--channels",
            "1",
            "--channel-map",
            "[ MONO ]",
            "--latency",
            "40",
        ])
        .arg(format!("--capture-props={}", props(from)))
        .arg(format!("--playback-props={}", props(to)))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        // If our app dies, don't leave audio flowing.
        .kill_on_drop(true);
    cmd
}

/// Read the current PipeWire graph.
async fn read_graph() -> Result<Graph> {
    let out = Command::new("pw-dump")
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| {
            Error::ServiceUnavailable(format!("cannot run pw-dump (install pipewire-bin): {e}"))
        })?;
    if !out.status.success() {
        return Err(Error::ServiceUnavailable(format!(
            "pw-dump failed (is PipeWire running?): {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Graph::parse(&String::from_utf8_lossy(&out.stdout))
        .map_err(|e| Error::Backend(format!("pw-dump output: {e}")))
}

/// The routing currently in place.
struct Route {
    phone: Address,
    nodes: PhoneNodes,
    /// The PC microphone we feed the phone with (None = PipeWire default).
    mic: Option<String>,
    speakers: Option<Child>,
    mic_loop: Option<Child>,
}

#[derive(Default)]
struct RouterState {
    route: Option<Route>,
    mic_muted: bool,
}

pub struct PipewireAudio {
    events: EventBus,
    state: Mutex<RouterState>,
}

impl PipewireAudio {
    pub fn new(_config: &PlatformConfig, events: EventBus) -> Self {
        Self {
            events,
            state: Mutex::new(RouterState::default()),
        }
    }

    /// Wait until the phone's call-audio nodes exist, asking the phone to open
    /// the audio link if they don't show up by themselves.
    async fn wait_for_nodes(phone: &Address) -> Result<(Graph, PhoneNodes)> {
        let deadline = tokio::time::Instant::now() + NODE_WAIT;
        let mut asked = false;
        loop {
            let graph = read_graph().await?;
            if let Some(nodes) = graph.phone_nodes(phone.as_str()) {
                return Ok((graph, nodes));
            }
            if !asked {
                asked = true;
                match crate::telephony::activate_audio_link(phone).await {
                    Ok(true) => tracing::info!("asked {phone} to open the call audio link"),
                    Ok(false) => {}
                    Err(e) => tracing::debug!("activate audio link: {e}"),
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(Error::InvalidState(format!(
                    "no call audio from {phone}: is a call active and audio set to this PC on the phone?"
                )));
            }
            tokio::time::sleep(NODE_POLL).await;
        }
    }
}

/// Start one loopback and check it didn't exit right away (bad arguments,
/// target node vanished...).
async fn spawn_loopback(
    direction: Direction,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<Child> {
    let mut child = loopback_command(direction, from, to).spawn().map_err(|e| {
        Error::ServiceUnavailable(format!(
            "cannot run pw-loopback (install pipewire-bin): {e}"
        ))
    })?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    match child.try_wait() {
        Ok(Some(status)) => Err(Error::Backend(format!(
            "pw-loopback ({}) exited: {status}",
            direction.node_name()
        ))),
        _ => Ok(child),
    }
}

/// End any `pw-loopback` we started, found by its `--name`. Returns `true`
/// if something was stopped. Best effort: `pkill` missing is not an error.
async fn kill_stray_loopbacks() -> bool {
    let pattern = format!(
        "pw-loopback --name ({}|{})",
        Direction::PhoneToSpeakers.node_name(),
        Direction::MicToPhone.node_name()
    );
    let status = Command::new("pkill").arg("-f").arg(pattern).status().await;
    matches!(status, Ok(s) if s.success())
}

async fn kill(child: Option<Child>) {
    if let Some(mut c) = child {
        // Already exited is fine.
        let _ = c.kill().await;
    }
}

#[async_trait]
impl AudioRouter for PipewireAudio {
    async fn start(&self, phone: &Address) -> Result<()> {
        let mut st = self.state.lock().await;
        if st.route.as_ref().is_some_and(|r| &r.phone == phone) {
            return Ok(());
        }
        if let Some(old) = st.route.take() {
            kill(old.speakers).await;
            kill(old.mic_loop).await;
        }

        let (graph, nodes) = Self::wait_for_nodes(phone).await?;
        let speakers_dev = graph
            .local_device(SINK, phone.as_str())
            .map(|n: &Node| n.name.clone());
        let mic_dev = graph
            .local_device(SOURCE, phone.as_str())
            .map(|n: &Node| n.name.clone());

        let speakers = if graph.linked_to_device(nodes.source.id, SINK) {
            tracing::info!("phone audio already goes to a sink; not adding a loopback");
            None
        } else {
            Some(
                spawn_loopback(
                    Direction::PhoneToSpeakers,
                    Some(&nodes.source.name),
                    speakers_dev.as_deref(),
                )
                .await?,
            )
        };
        let mic_loop = if st.mic_muted || graph.fed_by_device(nodes.sink.id, SOURCE) {
            None
        } else {
            match spawn_loopback(
                Direction::MicToPhone,
                mic_dev.as_deref(),
                Some(&nodes.sink.name),
            )
            .await
            {
                Ok(c) => Some(c),
                Err(e) => {
                    kill(speakers).await;
                    return Err(e);
                }
            }
        };

        st.route = Some(Route {
            phone: phone.clone(),
            nodes,
            mic: mic_dev,
            speakers,
            mic_loop,
        });
        drop(st);
        self.events.emit(PhoneEvent::AudioRouted { active: true });
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        let route = self.state.lock().await.route.take();
        let had_route = route.is_some();
        if let Some(r) = route {
            kill(r.speakers).await;
            kill(r.mic_loop).await;
        }
        // Also end loopbacks started by another phonesym process (e.g.
        // `phonesym call audio on` in another terminal).
        let stray = kill_stray_loopbacks().await;
        if had_route || stray {
            self.events.emit(PhoneEvent::AudioRouted { active: false });
        }
        Ok(())
    }

    async fn is_active(&self) -> bool {
        self.state.lock().await.route.is_some()
    }

    async fn set_mic_muted(&self, muted: bool) -> Result<()> {
        let mut st = self.state.lock().await;
        st.mic_muted = muted;
        let Some(route) = st.route.as_mut() else {
            return Ok(());
        };
        if muted {
            kill(route.mic_loop.take()).await;
        } else if route.mic_loop.is_none() {
            route.mic_loop = Some(
                spawn_loopback(
                    Direction::MicToPhone,
                    route.mic.as_deref(),
                    Some(&route.nodes.sink.name),
                )
                .await?,
            );
        }
        Ok(())
    }
}

/// Start/stop audio routing automatically as calls become active / end.
///
/// Rules: start when any call on a phone becomes `Active`; stop when that
/// phone has no calls left. Held calls keep the route (the other call is
/// usually active, and switching is instant).
pub fn spawn_auto_routing(audio: Arc<PipewireAudio>, events: EventBus) {
    let mut rx = events.subscribe();
    tokio::spawn(async move {
        // Call id -> (phone, state).
        let mut calls: HashMap<CallId, (Option<Address>, CallState)> = HashMap::new();
        loop {
            let ev = match rx.recv().await {
                Ok(ev) => ev,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            };
            match ev {
                PhoneEvent::CallAdded { call } | PhoneEvent::CallChanged { call } => {
                    let active = call.state == CallState::Active;
                    calls.insert(call.id.clone(), (call.device.clone(), call.state));
                    if active && !audio.is_active().await {
                        if let Some(phone) = call.device {
                            if let Err(e) = audio.start(&phone).await {
                                events.emit(PhoneEvent::Warning {
                                    message: format!("call audio: {e}"),
                                });
                            }
                        }
                    }
                }
                PhoneEvent::CallRemoved { id } => {
                    calls.remove(&id);
                    let ongoing = calls.values().any(|(_, s)| *s != CallState::Disconnected);
                    if !ongoing {
                        let _ = audio.stop().await;
                    }
                }
                PhoneEvent::CallsUnavailable { address } => {
                    calls.retain(|_, (dev, _)| dev.as_ref() != Some(&address));
                    if calls.is_empty() {
                        let _ = audio.stop().await;
                    }
                }
                _ => {}
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(cmd: &Command) -> Vec<String> {
        cmd.as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn loopback_targets_nodes() {
        let cmd = loopback_command(
            Direction::PhoneToSpeakers,
            Some("bluez_input.X.0"),
            Some("alsa_output.Y"),
        );
        let a = args(&cmd);
        assert_eq!(cmd.as_std().get_program(), "pw-loopback");
        assert!(a.contains(&"phonesym-phone-to-speakers".to_string()));
        assert!(a.contains(
            &r#"--capture-props={ target.object="bluez_input.X.0" node.dont-reconnect=true }"#
                .to_string()
        ));
        assert!(a.contains(
            &r#"--playback-props={ target.object="alsa_output.Y" node.dont-reconnect=true }"#
                .to_string()
        ));
    }

    #[test]
    fn loopback_without_local_device_uses_default() {
        let a = args(&loopback_command(
            Direction::MicToPhone,
            None,
            Some("bluez_output.X.0"),
        ));
        assert!(a.contains(&"--capture-props={ }".to_string()));
    }
}
