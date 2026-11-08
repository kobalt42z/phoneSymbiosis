//! bMessage: the text format MAP uses for whole messages (spec: Bluetooth MAP
//! 1.x, section "bMessage format").
//!
//! ```text
//! BEGIN:BMSG
//! VERSION:1.0
//! STATUS:UNREAD                <- READ / UNREAD
//! TYPE:SMS_GSM                 <- SMS_GSM, SMS_CDMA, MMS, EMAIL, IM
//! FOLDER:TELECOM/MSG/INBOX
//! BEGIN:VCARD ... END:VCARD    <- originator (0 or more)
//! BEGIN:BENV
//! BEGIN:VCARD ... END:VCARD    <- recipient(s)
//! BEGIN:BBODY
//! CHARSET:UTF-8
//! LENGTH:22                    <- see below
//! BEGIN:MSG
//! the text
//! END:MSG
//! END:BBODY
//! END:BENV
//! END:BMSG
//! ```
//!
//! **LENGTH rule.** `LENGTH` is the number of *bytes* of the message content
//! *including* the `BEGIN:MSG<CRLF>` line before the text and the
//! `<CRLF>END:MSG<CRLF>` after it, i.e. `11 + bytes(text) + 11`. That is how
//! the spec's examples count it and what Android's MAP implementation writes,
//! which iPhones and car kits accept.

use phonesym_core::vcard;

/// One party (sender or recipient) from a bMessage vCard.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Party {
    pub name: Option<String>,
    /// Phone number, or e-mail address if there is no number.
    pub address: Option<String>,
}

/// A parsed bMessage. Only the parts this app uses are kept.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BMessage {
    /// From `STATUS`: `Some(true)` = READ.
    pub read: Option<bool>,
    /// `TYPE`, e.g. `SMS_GSM`.
    pub msg_type: Option<String>,
    /// `FOLDER`, e.g. `TELECOM/MSG/INBOX` (empty for pushed messages).
    pub folder: Option<String>,
    pub originator: Option<Party>,
    pub recipients: Vec<Party>,
    /// `CHARSET` of the body (`UTF-8`, or `native` = undecoded PDU).
    pub charset: Option<String>,
    /// The message text. Several `BEGIN:MSG` parts (a long SMS split by the
    /// phone) are joined. Line breaks are `\n`.
    pub text: String,
}

const MSG_OPEN: &str = "BEGIN:MSG\r\n";
const MSG_CLOSE: &str = "\r\nEND:MSG\r\n";

/// Build the bMessage for an outgoing SMS to `number` (already validated).
///
/// - Line breaks in `text` are normalised to CRLF, like every other line.
/// - No originator vCard: it is optional in the grammar and the phone knows its own number.
/// - `FOLDER:` is left empty: the target folder is given by `PushMessage` itself.
/// - `STATUS:READ`: it's our own message, there is nothing for the phone to flag as new.
pub fn build_sms(number: &str, text: &str) -> String {
    let body = normalise_newlines(text);
    let length = MSG_OPEN.len() + body.len() + MSG_CLOSE.len();
    let mut out = String::with_capacity(256 + body.len());
    for line in [
        "BEGIN:BMSG",
        "VERSION:1.0",
        "STATUS:READ",
        "TYPE:SMS_GSM",
        "FOLDER:",
        "BEGIN:BENV",
        "BEGIN:VCARD",
        "VERSION:2.1",
        "N:",
    ] {
        out.push_str(line);
        out.push_str("\r\n");
    }
    out.push_str(&format!("TEL:{number}\r\n"));
    out.push_str("END:VCARD\r\nBEGIN:BBODY\r\nCHARSET:UTF-8\r\n");
    out.push_str(&format!("LENGTH:{length}\r\n"));
    out.push_str(MSG_OPEN);
    out.push_str(&body);
    out.push_str(MSG_CLOSE);
    out.push_str("END:BBODY\r\nEND:BENV\r\nEND:BMSG\r\n");
    out
}

/// Can this text go into a bMessage body? A line reading exactly `END:MSG`
/// would end the body early and the spec has no escape for it.
pub fn text_is_sendable(text: &str) -> bool {
    !text.lines().any(|l| l.trim_end_matches('\r') == "END:MSG")
}

fn normalise_newlines(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n")
}

/// Parse a bMessage downloaded with `Message1.Get`.
///
/// Forgiving like the vCard parser: unknown lines are ignored, LF-only line
/// endings work, and the text is taken between `BEGIN:MSG` and `END:MSG`
/// markers rather than trusting `LENGTH` (phones get it wrong surprisingly often).
/// Returns `None` if there is no `BEGIN:BMSG` at all.
pub fn parse(data: &str) -> Option<BMessage> {
    let mut msg = BMessage::default();
    let mut stack: Vec<String> = Vec::new();
    let mut vcard_lines: Option<Vec<&str>> = None;
    let mut parts: Vec<String> = Vec::new();
    let mut in_msg: Option<Vec<&str>> = None;
    let mut saw_bmsg = false;

    for raw in data.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);

        if let Some(body) = in_msg.as_mut() {
            if line == "END:MSG" {
                parts.push(body.join("\n"));
                in_msg = None;
            } else {
                body.push(line);
            }
            continue;
        }
        if let Some(lines) = vcard_lines.as_mut() {
            lines.push(line);
            if line.trim().eq_ignore_ascii_case("END:VCARD") {
                let text = lines.join("\r\n");
                vcard_lines = None;
                let party = vcard::parse_cards(&text).into_iter().next().map(|card| {
                    let name = Some(card.display_name()).filter(|n| !n.is_empty());
                    let address = card
                        .numbers()
                        .into_iter()
                        .next()
                        .map(|n| n.number)
                        .or_else(|| card.get("EMAIL").map(|p| p.text().trim().to_owned()))
                        .filter(|a| !a.is_empty());
                    Party { name, address }
                });
                if let Some(party) = party {
                    if stack.iter().any(|s| s == "BENV") {
                        msg.recipients.push(party);
                    } else if msg.originator.is_none() {
                        msg.originator = Some(party);
                    }
                }
            }
            continue;
        }

        let trimmed = line.trim();
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.to_ascii_uppercase();
        let value = value.trim();
        match key.as_str() {
            "BEGIN" => match value.to_ascii_uppercase().as_str() {
                "VCARD" => vcard_lines = Some(vec![trimmed]),
                "MSG" => in_msg = Some(Vec::new()),
                other => {
                    saw_bmsg |= other == "BMSG";
                    stack.push(other.to_owned());
                }
            },
            "END" => {
                stack.pop();
            }
            "STATUS" if stack.last().is_some_and(|s| s == "BMSG") => {
                msg.read = Some(value.eq_ignore_ascii_case("READ"));
            }
            "TYPE" if stack.last().is_some_and(|s| s == "BMSG") => {
                msg.msg_type = Some(value.to_owned())
            }
            "FOLDER" if stack.last().is_some_and(|s| s == "BMSG") => {
                msg.folder = Some(value.to_owned()).filter(|f| !f.is_empty())
            }
            "CHARSET" if msg.charset.is_none() => msg.charset = Some(value.to_owned()),
            _ => {}
        }
    }
    // A body cut off before END:MSG still beats nothing.
    if let Some(body) = in_msg {
        parts.push(body.join("\n"));
    }
    if !saw_bmsg {
        return None;
    }
    msg.text = parts.join("");
    Some(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_simple_sms_and_length() {
        let b = build_sms("+15550100", "Hi");
        assert_eq!(
            b,
            "BEGIN:BMSG\r\nVERSION:1.0\r\nSTATUS:READ\r\nTYPE:SMS_GSM\r\nFOLDER:\r\nBEGIN:BENV\r\n\
             BEGIN:VCARD\r\nVERSION:2.1\r\nN:\r\nTEL:+15550100\r\nEND:VCARD\r\nBEGIN:BBODY\r\n\
             CHARSET:UTF-8\r\nLENGTH:24\r\nBEGIN:MSG\r\nHi\r\nEND:MSG\r\nEND:BBODY\r\nEND:BENV\r\nEND:BMSG\r\n"
        );
    }

    /// LENGTH must equal the bytes from "BEGIN:MSG" through "END:MSG\r\n".
    fn check_length(b: &str) {
        let start = b.find("BEGIN:MSG\r\n").expect("has body");
        let end = b.find("\r\nEND:MSG\r\n").expect("has end") + MSG_CLOSE.len();
        let declared: usize = b
            .lines()
            .find_map(|l| l.strip_prefix("LENGTH:"))
            .and_then(|v| v.trim().parse().ok())
            .expect("has LENGTH");
        assert_eq!(declared, end - start);
    }

    #[test]
    fn length_counts_utf8_bytes_and_crlf() {
        for text in ["", "Hi", "Grüße 👋", "line1\nline2\r\nline3", "a\rb"] {
            let b = build_sms("123", text);
            check_length(&b);
        }
        assert!(build_sms("1", "a\nb").contains("BEGIN:MSG\r\na\r\nb\r\nEND:MSG"));
        assert!(build_sms("1", "Grüße").contains("LENGTH:29\r\n")); // 22 + 7 bytes
    }

    #[test]
    fn roundtrip() {
        let text = "Hello\nWorld ✓";
        let parsed = parse(&build_sms("+33 1", text)).expect("parses");
        assert_eq!(parsed.text, text);
        assert_eq!(
            parsed.recipients,
            vec![Party {
                name: None,
                address: Some("+33 1".into())
            }]
        );
        assert_eq!(parsed.originator, None);
        assert_eq!(parsed.read, Some(true));
        assert_eq!(parsed.charset.as_deref(), Some("UTF-8"));
    }

    #[test]
    fn sendable_text() {
        assert!(text_is_sendable("hello END:MSG"));
        assert!(!text_is_sendable("a\nEND:MSG\nb"));
    }

    #[test]
    fn parse_incoming_from_phone() {
        let data = "BEGIN:BMSG\r\nVERSION:1.0\r\nSTATUS:UNREAD\r\nTYPE:SMS_GSM\r\nFOLDER:TELECOM/MSG/INBOX\r\n\
                    BEGIN:VCARD\r\nVERSION:2.1\r\nN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:M=C3=BCller;Anna\r\n\
                    TEL:+491701234\r\nEND:VCARD\r\nBEGIN:BENV\r\nBEGIN:VCARD\r\nVERSION:2.1\r\nN:\r\nTEL:\r\nEND:VCARD\r\n\
                    BEGIN:BBODY\r\nCHARSET:UTF-8\r\nLENGTH:44\r\nBEGIN:MSG\r\nSee you at 5?\r\nBring snacks\r\nEND:MSG\r\n\
                    END:BBODY\r\nEND:BENV\r\nEND:BMSG\r\n";
        let m = parse(data).expect("parses");
        assert_eq!(m.read, Some(false));
        assert_eq!(m.msg_type.as_deref(), Some("SMS_GSM"));
        assert_eq!(m.folder.as_deref(), Some("TELECOM/MSG/INBOX"));
        assert_eq!(
            m.originator,
            Some(Party {
                name: Some("Anna Müller".into()),
                address: Some("+491701234".into())
            })
        );
        assert_eq!(
            m.recipients,
            vec![Party {
                name: None,
                address: None
            }]
        );
        assert_eq!(m.text, "See you at 5?\nBring snacks");
    }

    #[test]
    fn parse_multipart_lf_only_and_text_looking_like_keywords() {
        let data = "BEGIN:BMSG\nVERSION:1.0\nSTATUS:READ\nTYPE:SMS_GSM\nFOLDER:\nBEGIN:BENV\nBEGIN:BBODY\n\
                    CHARSET:UTF-8\nLENGTH:1\nBEGIN:MSG\nPart one, \nEND:MSG\nBEGIN:MSG\nSTATUS:fake\nEND:BMSG is text\nEND:MSG\n\
                    END:BBODY\nEND:BENV\nEND:BMSG\n";
        let m = parse(data).expect("parses");
        assert_eq!(m.text, "Part one, STATUS:fake\nEND:BMSG is text");
        assert_eq!(m.read, Some(true));
        assert_eq!(m.folder, None);
    }

    #[test]
    fn parse_truncated_and_invalid() {
        let m =
            parse("BEGIN:BMSG\nBEGIN:BENV\nBEGIN:BBODY\nBEGIN:MSG\nhalf a mess").expect("parses");
        assert_eq!(m.text, "half a mess");
        assert_eq!(parse("not a bmessage"), None);
        assert_eq!(parse(""), None);
    }
}
