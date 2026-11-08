//! Understanding the output of `pw-dump` (a JSON array of every PipeWire
//! object) well enough to find the phone's call-audio nodes.
//!
//! During an HFP call PipeWire's Bluetooth plugin creates two nodes for the
//! phone, both with `api.bluez5.address` = the phone address and
//! `api.bluez5.profile` = `headset-audio-gateway` (the phone's role):
//! - `Audio/Source`: the caller's voice coming from the phone,
//! - `Audio/Sink`: what we send to the phone (our microphone).

use serde_json::Value;

/// The few node properties we care about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: u64,
    pub name: String,
    pub media_class: String,
    pub bluez_address: Option<String>,
    pub bluez_profile: Option<String>,
    pub priority: i64,
}

/// A link between two nodes (made by WirePlumber, us, or the user).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub output_node: u64,
    pub input_node: u64,
}

/// Parsed `pw-dump` output.
#[derive(Debug, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    pub default_sink: Option<String>,
    pub default_source: Option<String>,
}

/// The phone's two call-audio nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhoneNodes {
    /// Voice from the phone.
    pub source: Node,
    /// Audio to the phone.
    pub sink: Node,
}

pub const SOURCE: &str = "Audio/Source";
pub const SINK: &str = "Audio/Sink";

impl Graph {
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        let objects: Vec<Value> = serde_json::from_str(json)?;
        let mut graph = Graph::default();
        for obj in &objects {
            match obj.get("type").and_then(Value::as_str) {
                Some("PipeWire:Interface:Node") => graph.nodes.extend(parse_node(obj)),
                Some("PipeWire:Interface:Link") => graph.links.extend(parse_link(obj)),
                Some("PipeWire:Interface:Metadata") => graph.read_defaults(obj),
                _ => {}
            }
        }
        Ok(graph)
    }

    /// `default.audio.sink` / `default.audio.source` from the "default"
    /// metadata object (what the desktop sound settings show as selected).
    fn read_defaults(&mut self, obj: &Value) {
        let is_default =
            obj.pointer("/props/metadata.name").and_then(Value::as_str) == Some("default");
        let Some(entries) = obj
            .get("metadata")
            .and_then(Value::as_array)
            .filter(|_| is_default)
        else {
            return;
        };
        for e in entries {
            let name = e.get("value").and_then(metadata_name);
            match e.get("key").and_then(Value::as_str) {
                Some("default.audio.sink") => self.default_sink = name,
                Some("default.audio.source") => self.default_source = name,
                _ => {}
            }
        }
    }

    pub fn node(&self, id: u64) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Find the phone's call-audio nodes, if the call audio link is up.
    pub fn phone_nodes(&self, address: &str) -> Option<PhoneNodes> {
        let pick = |class: &str| {
            self.nodes
                .iter()
                .filter(|n| n.media_class == class && n.is_call_audio_of(address))
                .max_by_key(|n| n.priority)
                .cloned()
        };
        Some(PhoneNodes {
            source: pick(SOURCE)?,
            sink: pick(SINK)?,
        })
    }

    /// The PC's speakers or microphone: the desktop default, unless the
    /// default *is* the phone (WirePlumber may pick a Bluetooth node as
    /// default), in which case the best-ranked other device.
    pub fn local_device(&self, class: &str, phone_address: &str) -> Option<&Node> {
        let usable = |n: &&Node| n.media_class == class && !n.is_device_of(phone_address);
        let default = match class {
            SINK => self.default_sink.as_deref(),
            _ => self.default_source.as_deref(),
        };
        default
            .and_then(|name| self.nodes.iter().filter(usable).find(|n| n.name == name))
            .or_else(|| self.nodes.iter().filter(usable).max_by_key(|n| n.priority))
    }

    /// Is `from` already linked to some audio device (not just a stream)?
    /// Then someone (WirePlumber policy or the user) already routes it.
    pub fn linked_to_device(&self, from: u64, to_class: &str) -> bool {
        self.links
            .iter()
            .filter(|l| l.output_node == from)
            .filter_map(|l| self.node(l.input_node))
            .any(|n| n.media_class == to_class)
    }

    /// Is some audio device already feeding `to`?
    pub fn fed_by_device(&self, to: u64, from_class: &str) -> bool {
        self.links
            .iter()
            .filter(|l| l.input_node == to)
            .filter_map(|l| self.node(l.output_node))
            .any(|n| n.media_class == from_class)
    }
}

impl Node {
    fn is_device_of(&self, address: &str) -> bool {
        self.bluez_address
            .as_deref()
            .is_some_and(|a| a.eq_ignore_ascii_case(address))
    }

    /// Hands-free (SCO) node of this phone, as opposed to e.g. its A2DP
    /// music stream, which has the same address.
    fn is_call_audio_of(&self, address: &str) -> bool {
        self.is_device_of(address)
            && self
                .bluez_profile
                .as_deref()
                .is_none_or(|p| p.starts_with("headset"))
    }
}

fn parse_node(obj: &Value) -> Option<Node> {
    let props = obj.pointer("/info/props")?;
    let s = |k: &str| props.get(k).and_then(Value::as_str).map(str::to_owned);
    Some(Node {
        id: obj.get("id")?.as_u64()?,
        name: s("node.name")?,
        media_class: s("media.class").unwrap_or_default(),
        bluez_address: s("api.bluez5.address"),
        bluez_profile: s("api.bluez5.profile"),
        priority: props
            .get("priority.session")
            .and_then(Value::as_i64)
            .unwrap_or(0),
    })
}

fn parse_link(obj: &Value) -> Option<Link> {
    let info = obj.get("info")?;
    Some(Link {
        output_node: info.get("output-node-id")?.as_u64()?,
        input_node: info.get("input-node-id")?.as_u64()?,
    })
}

/// Metadata values look like `{"name": "alsa_output..."}`; older tools
/// print the same JSON as a string.
fn metadata_name(v: &Value) -> Option<String> {
    match v {
        Value::Object(_) => v.get("name").and_then(Value::as_str).map(str::to_owned),
        Value::String(s) => serde_json::from_str::<Value>(s)
            .ok()
            .as_ref()
            .and_then(metadata_name),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHONE: &str = "A4:83:E7:12:34:56";

    /// Trimmed but structurally faithful `pw-dump` output during a call,
    /// with the iPhone also connected for music (A2DP).
    const SAMPLE: &str = r#"[
      { "id": 0, "type": "PipeWire:Interface:Core", "version": 4, "permissions": ["r","w","x","m"],
        "info": { "name": "pipewire-0", "props": { "core.name": "pipewire-0" } } },
      { "id": 33, "type": "PipeWire:Interface:Metadata", "version": 3,
        "props": { "metadata.name": "default" },
        "metadata": [
          { "subject": 0, "key": "default.configured.audio.sink", "type": "Spa:String:JSON", "value": { "name": "alsa_output.pci-0000_00_1f.3.analog-stereo" } },
          { "subject": 0, "key": "default.audio.sink", "type": "Spa:String:JSON", "value": { "name": "alsa_output.pci-0000_00_1f.3.analog-stereo" } },
          { "subject": 0, "key": "default.audio.source", "type": "Spa:String:JSON", "value": { "name": "bluez_input.A4_83_E7_12_34_56.0" } }
        ] },
      { "id": 34, "type": "PipeWire:Interface:Metadata", "props": { "metadata.name": "settings" },
        "metadata": [ { "subject": 0, "key": "clock.rate", "type": "", "value": 48000 } ] },
      { "id": 48, "type": "PipeWire:Interface:Node", "version": 3,
        "info": { "state": "suspended", "props": {
          "node.name": "alsa_output.pci-0000_00_1f.3.analog-stereo", "media.class": "Audio/Sink",
          "device.api": "alsa", "priority.session": 1009, "object.serial": 48 } } },
      { "id": 49, "type": "PipeWire:Interface:Node", "version": 3,
        "info": { "state": "suspended", "props": {
          "node.name": "alsa_input.pci-0000_00_1f.3.analog-stereo", "media.class": "Audio/Source",
          "device.api": "alsa", "priority.session": 2009, "object.serial": 49 } } },
      { "id": 77, "type": "PipeWire:Interface:Node", "version": 3,
        "info": { "state": "running", "props": {
          "node.name": "bluez_input.A4_83_E7_12_34_56.2", "media.class": "Audio/Source",
          "api.bluez5.address": "A4:83:E7:12:34:56", "api.bluez5.profile": "a2dp-source",
          "api.bluez5.codec": "aac", "device.api": "bluez5", "priority.session": 2010 } } },
      { "id": 81, "type": "PipeWire:Interface:Node", "version": 3,
        "info": { "state": "idle", "props": {
          "node.name": "bluez_input.A4_83_E7_12_34_56.0", "media.class": "Audio/Source",
          "api.bluez5.address": "A4:83:E7:12:34:56", "api.bluez5.profile": "headset-audio-gateway",
          "api.bluez5.codec": "msbc", "api.bluez5.transport": "12", "device.api": "bluez5",
          "priority.session": 2010 } } },
      { "id": 82, "type": "PipeWire:Interface:Node", "version": 3,
        "info": { "state": "idle", "props": {
          "node.name": "bluez_output.A4_83_E7_12_34_56.0", "media.class": "Audio/Sink",
          "api.bluez5.address": "A4:83:E7:12:34:56", "api.bluez5.profile": "headset-audio-gateway",
          "api.bluez5.codec": "msbc", "device.api": "bluez5", "priority.session": 1010 } } },
      { "id": 90, "type": "PipeWire:Interface:Node", "version": 3,
        "info": { "props": { "node.name": "Firefox", "media.class": "Stream/Output/Audio" } } },
      { "id": 95, "type": "PipeWire:Interface:Link", "version": 3,
        "info": { "output-node-id": 90, "output-port-id": 91, "input-node-id": 48, "input-port-id": 50, "state": "active" } }
    ]"#;

    #[test]
    fn finds_phone_call_nodes_not_music() {
        let g = Graph::parse(SAMPLE).unwrap();
        let p = g.phone_nodes(&PHONE.to_lowercase()).expect("phone nodes");
        assert_eq!(p.source.id, 81);
        assert_eq!(p.source.name, "bluez_input.A4_83_E7_12_34_56.0");
        assert_eq!(p.sink.id, 82);
        assert!(g.phone_nodes("00:00:00:00:00:01").is_none());
    }

    #[test]
    fn local_devices_skip_the_phone() {
        let g = Graph::parse(SAMPLE).unwrap();
        assert_eq!(
            g.default_sink.as_deref(),
            Some("alsa_output.pci-0000_00_1f.3.analog-stereo")
        );
        assert_eq!(g.local_device(SINK, PHONE).map(|n| n.id), Some(48));
        // The default source is the phone itself, so we must not use it.
        assert_eq!(g.local_device(SOURCE, PHONE).map(|n| n.id), Some(49));
    }

    #[test]
    fn link_detection() {
        let g = Graph::parse(SAMPLE).unwrap();
        assert!(!g.linked_to_device(81, SINK));
        assert!(!g.fed_by_device(82, SOURCE));
        // A stream feeding the sink is not a device link.
        assert!(!g.fed_by_device(48, SOURCE));
        let mut g = g;
        g.links.push(Link {
            output_node: 81,
            input_node: 48,
        });
        assert!(g.linked_to_device(81, SINK));
    }

    #[test]
    fn metadata_value_as_string() {
        let v = Value::String(r#"{ "name": "x" }"#.into());
        assert_eq!(metadata_name(&v).as_deref(), Some("x"));
    }

    #[test]
    fn bad_json_is_an_error() {
        assert!(Graph::parse("not json").is_err());
        assert!(Graph::parse("[]").unwrap().nodes.is_empty());
    }
}
