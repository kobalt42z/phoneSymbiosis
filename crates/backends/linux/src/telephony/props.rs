//! Pure helpers for telephony: parsing call properties, validating input,
//! converting volume scales. No D-Bus calls here, so all of it is unit tested.

use crate::dbus_util::{get_bool, get_str, get_u8_lenient, PropMap};
use phonesym_core::{Address, Call, CallDirection, CallId, CallState, CallVolume, Error, Result};

/// Characters a dial string may contain, per PipeWire telephony / HFP `ATD`.
const DIAL_CHARS: &str = "0123456789+*#,ABCD";
/// Characters a DTMF tone string may contain.
const TONE_CHARS: &str = "0123456789*#ABCD";
const MAX_NUMBER_LEN: usize = 80;

/// Clean up a phone number the way a person types it ("+33 6 12-34-56-78",
/// "(555) 123.4567") and check it against `[0-9+*#,ABCD]{1,80}`.
pub fn normalize_number(input: &str) -> Result<String> {
    let cleaned: String = input
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '(' | ')' | '.' | '\u{a0}'))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if cleaned.is_empty() || cleaned.len() > MAX_NUMBER_LEN {
        return Err(Error::InvalidArgument(format!(
            "phone number must have 1 to {MAX_NUMBER_LEN} digits: {input:?}"
        )));
    }
    if let Some(bad) = input.chars().find(|c| {
        !c.is_whitespace() && !"-().".contains(*c) && !DIAL_CHARS.contains(c.to_ascii_uppercase())
    }) {
        return Err(Error::InvalidArgument(format!(
            "invalid character {bad:?} in phone number {input:?}"
        )));
    }
    Ok(cleaned)
}

/// Check DTMF tones (`0-9 * # A-D`).
pub fn normalize_tones(input: &str) -> Result<String> {
    let cleaned: String = input
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if cleaned.is_empty() {
        return Err(Error::InvalidArgument("no DTMF tones given".into()));
    }
    if let Some(bad) = input
        .chars()
        .find(|c| !c.is_whitespace() && !TONE_CHARS.contains(c.to_ascii_uppercase()))
    {
        return Err(Error::InvalidArgument(format!("invalid DTMF tone {bad:?}")));
    }
    Ok(cleaned)
}

/// The services don't report call direction, so guess it from the first
/// state we saw: ringing means incoming, dialing/alerting means outgoing.
/// For a call already in progress when we started, the presence of
/// `IncomingLine` (only set for incoming calls) is the best hint.
pub fn infer_direction(state: CallState, has_incoming_line: bool) -> CallDirection {
    match state {
        CallState::Incoming | CallState::Waiting => CallDirection::Incoming,
        CallState::Dialing | CallState::Alerting => CallDirection::Outgoing,
        _ if has_incoming_line => CallDirection::Incoming,
        _ => CallDirection::Outgoing,
    }
}

/// Build a [`Call`] from a call object's properties (same keys on
/// `org.pipewire.Telephony.Call1` and `org.ofono.VoiceCall`).
///
/// `known` is our previous view of the call, so the direction stays what we
/// first inferred instead of flipping when the state changes.
pub fn call_from_props(
    path: &str,
    props: &PropMap,
    device: Option<Address>,
    known: Option<&Call>,
) -> Option<Call> {
    let state = CallState::from_hfp_str(&get_str(props, "State")?)?;
    let incoming_line = get_str(props, "IncomingLine").filter(|s| !s.is_empty());
    let direction = known
        .map(|c| c.direction)
        .unwrap_or_else(|| infer_direction(state, incoming_line.is_some()));
    Some(Call {
        id: CallId(path.to_owned()),
        device,
        number: get_str(props, "LineIdentification").unwrap_or_default(),
        name: get_str(props, "Name").filter(|s| !s.is_empty()),
        state,
        direction,
        multiparty: get_bool(props, "Multiparty").unwrap_or(false),
    })
}

/// Apply a partial property update to a known call. Returns `true` if
/// something we care about changed.
pub fn apply_call_update(call: &mut Call, changed: &PropMap) -> bool {
    let before = (
        call.state,
        call.number.clone(),
        call.name.clone(),
        call.multiparty,
    );
    if let Some(state) = get_str(changed, "State").and_then(|s| CallState::from_hfp_str(&s)) {
        call.state = state;
    }
    if let Some(n) = get_str(changed, "LineIdentification") {
        call.number = n;
    }
    if let Some(n) = get_str(changed, "Name") {
        call.name = Some(n).filter(|s| !s.is_empty());
    }
    if let Some(m) = get_bool(changed, "Multiparty") {
        call.multiparty = m;
    }
    before
        != (
            call.state,
            call.number.clone(),
            call.name.clone(),
            call.multiparty,
        )
}

/// Call objects live right below their phone: `/…/ag0/call1` -> `/…/ag0`.
pub fn gateway_of_call(call_path: &str) -> Option<&str> {
    call_path
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .filter(|p| !p.is_empty())
}

/// HFP volume is 0-15; the oFono `CallVolume` interface uses 0-100 %.
/// Same rounding as PipeWire's compat layer.
pub fn percent_to_hfp(pct: u8) -> u8 {
    ((u16::from(pct.min(100)) * 15 + 50) / 100) as u8
}

pub fn hfp_to_percent(v: u8) -> u8 {
    ((u16::from(v.min(15)) * 100 + 7) / 15) as u8
}

/// Read `SpeakerVolume` / `MicrophoneVolume` (0-15, or 0-100 % when
/// `percent` is set, as on `org.ofono.CallVolume`) into a [`CallVolume`].
/// A missing key keeps the value from `base`.
pub fn volume_update(base: CallVolume, props: &PropMap, percent: bool) -> CallVolume {
    let read = |key| {
        get_u8_lenient(props, key).map(|v| {
            if percent {
                percent_to_hfp(v)
            } else {
                v.min(15)
            }
        })
    };
    CallVolume {
        speaker: read("SpeakerVolume").unwrap_or(base.speaker),
        microphone: read("MicrophoneVolume").unwrap_or(base.microphone),
    }
}

pub fn volume_from_props(props: &PropMap, percent: bool) -> CallVolume {
    volume_update(
        CallVolume {
            speaker: 0,
            microphone: 0,
        },
        props,
        percent,
    )
}

/// Does the oFono-style `GetModems` entry offer voice calls? Real oFono lists
/// every modem; only those with `VoiceCallManager` can make calls. PipeWire
/// always includes it. No `Interfaces` key at all: assume yes.
pub fn modem_has_voice(props: &PropMap) -> bool {
    match props.get("Interfaces") {
        None => true,
        Some(_) => crate::dbus_util::get_str_list(props, "Interfaces")
            .iter()
            .any(|i| i == super::service::OFONO_VOICE_CALL_MANAGER),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{OwnedValue, Value};

    fn v<'a>(x: impl Into<Value<'a>>) -> OwnedValue {
        x.into().try_to_owned().expect("owned")
    }

    fn props(pairs: &[(&str, OwnedValue)]) -> PropMap {
        pairs
            .iter()
            .map(|(k, val)| (k.to_string(), val.try_clone().unwrap()))
            .collect()
    }

    #[test]
    fn numbers() {
        assert_eq!(
            normalize_number("+33 6 12-34-56-78").unwrap(),
            "+33612345678"
        );
        assert_eq!(normalize_number("(555) 123.4567").unwrap(), "5551234567");
        assert_eq!(normalize_number("*#06#").unwrap(), "*#06#");
        assert_eq!(normalize_number("123,456abc").unwrap(), "123,456ABC");
        assert!(normalize_number("").is_err());
        assert!(normalize_number("  ").is_err());
        assert!(normalize_number("12x4").is_err());
        assert!(normalize_number("1E").is_err());
        assert!(normalize_number(&"1".repeat(80)).is_ok());
        assert!(normalize_number(&"1".repeat(81)).is_err());
    }

    #[test]
    fn tones() {
        assert_eq!(normalize_tones("1 2 3#").unwrap(), "123#");
        assert_eq!(normalize_tones("abcd*").unwrap(), "ABCD*");
        assert!(normalize_tones("+").is_err());
        assert!(normalize_tones(",").is_err());
        assert!(normalize_tones("").is_err());
    }

    #[test]
    fn parses_incoming_call() {
        let p = props(&[
            ("State", v("incoming")),
            ("LineIdentification", v("+15551234567")),
            ("Name", v("")),
            ("Multiparty", v(false)),
        ]);
        let addr = Address::parse("A4:83:E7:12:34:56").ok();
        let c =
            call_from_props("/org/pipewire/Telephony/ag0/call0", &p, addr.clone(), None).unwrap();
        assert_eq!(c.id.0, "/org/pipewire/Telephony/ag0/call0");
        assert_eq!(c.state, CallState::Incoming);
        assert_eq!(c.direction, CallDirection::Incoming);
        assert_eq!(c.number, "+15551234567");
        assert_eq!(c.name, None);
        assert_eq!(c.device, addr);
    }

    #[test]
    fn direction_is_sticky_and_updates_apply() {
        let p = props(&[("State", v("dialing")), ("LineIdentification", v("123"))]);
        let mut c = call_from_props("/x/ag0/call1", &p, None, None).unwrap();
        assert_eq!(c.direction, CallDirection::Outgoing);

        assert!(apply_call_update(&mut c, &props(&[("State", v("active"))])));
        assert_eq!(c.state, CallState::Active);
        assert!(!apply_call_update(
            &mut c,
            &props(&[("State", v("active"))])
        ));
        assert!(apply_call_update(
            &mut c,
            &props(&[("Multiparty", v(true))])
        ));

        let again = call_from_props(
            "/x/ag0/call1",
            &props(&[("State", v("active"))]),
            None,
            Some(&c),
        )
        .unwrap();
        assert_eq!(again.direction, CallDirection::Outgoing);
    }

    #[test]
    fn unknown_state_is_skipped() {
        assert!(call_from_props("/a/b", &props(&[("State", v("bogus"))]), None, None).is_none());
        assert!(call_from_props("/a/b", &PropMap::new(), None, None).is_none());
    }

    #[test]
    fn active_call_found_at_startup() {
        assert_eq!(
            infer_direction(CallState::Active, true),
            CallDirection::Incoming
        );
        assert_eq!(
            infer_direction(CallState::Active, false),
            CallDirection::Outgoing
        );
    }

    #[test]
    fn gateway_path() {
        assert_eq!(
            gateway_of_call("/org/pipewire/Telephony/ag0/call1"),
            Some("/org/pipewire/Telephony/ag0")
        );
        assert_eq!(
            gateway_of_call("/hfp/org/bluez/hci0/dev_X/voicecall01"),
            Some("/hfp/org/bluez/hci0/dev_X")
        );
        assert_eq!(gateway_of_call("/call"), None);
    }

    #[test]
    fn volume_scales() {
        assert_eq!(percent_to_hfp(0), 0);
        assert_eq!(percent_to_hfp(100), 15);
        assert_eq!(percent_to_hfp(200), 15);
        assert_eq!(hfp_to_percent(15), 100);
        assert_eq!(hfp_to_percent(0), 0);
        for v in 0..=15 {
            assert_eq!(percent_to_hfp(hfp_to_percent(v)), v);
        }
    }

    #[test]
    fn volume_props() {
        let v1 = volume_from_props(
            &props(&[("SpeakerVolume", v(15u8)), ("MicrophoneVolume", v(7u8))]),
            false,
        );
        assert_eq!((v1.speaker, v1.microphone), (15, 7));
        let pct = volume_from_props(
            &props(&[("SpeakerVolume", v(100u8)), ("MicrophoneVolume", v(50u8))]),
            true,
        );
        assert_eq!((pct.speaker, pct.microphone), (15, 8));
        let partial = volume_update(v1, &props(&[("SpeakerVolume", v(3u8))]), false);
        assert_eq!((partial.speaker, partial.microphone), (3, 7));
    }

    #[test]
    fn voice_capable_modems() {
        assert!(modem_has_voice(&PropMap::new()));
        let yes = props(&[(
            "Interfaces",
            v(vec!["org.ofono.VoiceCallManager".to_string()]),
        )]);
        assert!(modem_has_voice(&yes));
        let no = props(&[("Interfaces", v(vec!["org.ofono.SimManager".to_string()]))]);
        assert!(!modem_has_voice(&no));
    }
}
