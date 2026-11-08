//! Minimal vCard (2.1 / 3.0) parser for phonebook downloads.
//!
//! Phones send their address book (PBAP) and the vCards inside SMS envelopes
//! (MAP bMessages) in this format. iPhones in particular often answer with
//! vCard **2.1**, which brings a few old quirks this parser handles:
//!
//! - **Folded lines**: a long line is split and the continuation starts with
//!   a space or tab (both versions).
//! - **QUOTED-PRINTABLE**: non-ASCII text such as `FN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:Jos=C3=A9`
//!   with "soft" line breaks (a line ending in `=` continues on the next one).
//! - **Bare parameters**: 2.1 writes `TEL;CELL;VOICE:...` where 3.0 writes `TEL;TYPE=cell,voice:...`.
//! - **BASE64 photos** that span many lines, sometimes without indentation.
//!
//! The parser is forgiving on purpose: a malformed line is skipped instead of
//! failing the whole download, because one odd contact should never hide the
//! rest of the address book.
//!
//! Layers, from low to high:
//! 1. [`parse_cards`] turns text into generic [`Card`]s made of [`Property`]s.
//! 2. [`parse_contacts`] / [`parse_call_history`] map cards to the app's model types.

use crate::model::{CallHistoryEntry, CallHistoryKind, Contact, PhoneNumber};

// ---------------------------------------------------------------------------
// Generic layer: cards and properties
// ---------------------------------------------------------------------------

/// One `NAME;PARAMS:VALUE` line of a vCard, already unfolded and decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    /// Optional group prefix, e.g. `item1` in `item1.TEL:...` (Apple uses these for labels).
    pub group: Option<String>,
    /// Property name in upper case (`TEL`, `FN`, ...).
    pub name: String,
    /// Parameters as `(NAME, value)`, name upper case, value as written (quotes removed).
    /// Comma lists (`TYPE=cell,voice`) become one entry per value; 2.1 bare
    /// parameters (`;CELL`) become `("TYPE", "CELL")` or `("ENCODING", ...)`.
    pub params: Vec<(String, String)>,
    /// Value after transfer decoding (quoted-printable and charset are undone,
    /// base64 is kept as base64 with whitespace removed). vCard backslash
    /// escapes are **not** removed yet: use [`Property::text`] or [`Property::components`].
    pub value: String,
}

impl Property {
    /// All values of the parameter `name` (case-insensitive).
    pub fn param_values<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.params
            .iter()
            .filter(move |(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// `TYPE` values in lower case, e.g. `["cell", "voice"]`.
    pub fn types(&self) -> Vec<String> {
        self.param_values("TYPE")
            .map(str::to_ascii_lowercase)
            .collect()
    }

    /// True if any parameter name or value equals `word` (case-insensitive).
    /// Handy for 2.1 flags that can show up in either position.
    pub fn has_flag(&self, word: &str) -> bool {
        self.params
            .iter()
            .any(|(n, v)| n.eq_ignore_ascii_case(word) || v.eq_ignore_ascii_case(word))
    }

    /// The value as plain text with vCard escapes (`\n`, `\,`, `\;`, `\\`) removed.
    pub fn text(&self) -> String {
        unescape(&self.value)
    }

    /// The value split on unescaped `;` (structured values such as `N` and `ORG`),
    /// each part unescaped.
    pub fn components(&self) -> Vec<String> {
        split_unescaped(&self.value, ';')
            .iter()
            .map(|p| unescape(p))
            .collect()
    }
}

/// One `BEGIN:VCARD` ... `END:VCARD` block.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Card {
    /// The `VERSION` value (`2.1`, `3.0`, ...), if present.
    pub version: Option<String>,
    /// Properties in file order, excluding `BEGIN`, `END` and `VERSION`.
    pub properties: Vec<Property>,
}

impl Card {
    /// First property called `name` (case-insensitive).
    pub fn get(&self, name: &str) -> Option<&Property> {
        self.properties
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Every property called `name` (case-insensitive), in file order.
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Property> + 'a {
        self.properties
            .iter()
            .filter(move |p| p.name.eq_ignore_ascii_case(name))
    }

    /// Best human name for the card: `FN`, else a name built from `N`.
    /// Empty if the card has neither (callers choose their own fallback).
    pub fn display_name(&self) -> String {
        if let Some(fname) = self.get("FN").map(|p| p.text().trim().to_owned()) {
            if !fname.is_empty() {
                return fname;
            }
        }
        self.get("N")
            .map(format_structured_name)
            .unwrap_or_default()
    }

    /// Phone numbers from all `TEL` properties, skipping empty ones.
    pub fn numbers(&self) -> Vec<PhoneNumber> {
        self.all("TEL")
            .filter_map(|p| {
                let number = clean_number(&p.text());
                if number.is_empty() {
                    return None;
                }
                Some(PhoneNumber {
                    number,
                    kind: number_kind(p),
                })
            })
            .collect()
    }
}

/// Split a file holding one or more vCards into [`Card`]s.
///
/// Text outside `BEGIN:VCARD`/`END:VCARD` is ignored. Nested cards (2.1
/// `AGENT`) are skipped. A card missing its `END:VCARD` at the end of the
/// input is still returned, since truncated downloads happen.
pub fn parse_cards(data: &str) -> Vec<Card> {
    let mut cards = Vec::new();
    let mut current: Option<Card> = None;
    // Depth of nested BEGIN:VCARD inside the current card (AGENT cards).
    let mut nested = 0usize;

    for line in logical_lines(data) {
        let Some(prop) = parse_line(&line) else {
            continue;
        };
        let is_vcard = prop.value.trim().eq_ignore_ascii_case("VCARD");
        match prop.name.as_str() {
            "BEGIN" if is_vcard => {
                if current.is_some() {
                    nested += 1;
                } else {
                    current = Some(Card::default());
                }
            }
            "END" if is_vcard => {
                if nested > 0 {
                    nested -= 1;
                } else if let Some(card) = current.take() {
                    cards.push(card);
                }
            }
            _ if nested > 0 => {}
            "VERSION" => {
                if let Some(card) = current.as_mut() {
                    card.version = Some(prop.value.trim().to_owned());
                }
            }
            _ => {
                if let Some(card) = current.as_mut() {
                    card.properties.push(prop);
                }
            }
        }
    }
    if let Some(card) = current {
        cards.push(card);
    }
    cards
}

// ---------------------------------------------------------------------------
// Model layer: contacts and call history
// ---------------------------------------------------------------------------

/// Parse a phonebook file with one or more vCards.
///
/// - `id` is the card's `UID` / `X-BT-UID` if present, otherwise its position
///   in the file. In a PBAP `PullAll` the position matches the phone's handle
///   (`0.vcf` is the owner's own card).
/// - `name` falls back from `FN` to `N`, then `ORG`, e-mail and number, so a
///   list never shows blank rows.
/// - Completely empty cards (iPhones send an empty owner card) are dropped.
pub fn parse_contacts(data: &str) -> Vec<Contact> {
    parse_cards(data)
        .into_iter()
        .enumerate()
        .filter_map(|(index, card)| {
            let numbers = card.numbers();
            let emails: Vec<String> = card
                .all("EMAIL")
                .map(|p| p.text().trim().to_owned())
                .filter(|e| !e.is_empty())
                .collect();
            let organization = card.get("ORG").and_then(|p| {
                let parts: Vec<String> = p
                    .components()
                    .into_iter()
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect();
                (!parts.is_empty()).then(|| parts.join(", "))
            });
            let photo_base64 = card.get("PHOTO").and_then(photo_base64);

            let mut name = card.display_name();
            if name.is_empty() {
                name = organization
                    .clone()
                    .or_else(|| emails.first().cloned())
                    .or_else(|| numbers.first().map(|n| n.number.clone()))
                    .unwrap_or_default();
            }
            if name.is_empty() && numbers.is_empty() && emails.is_empty() {
                return None;
            }

            let id = card
                .get("X-BT-UID")
                .or_else(|| card.get("UID"))
                .map(|p| p.text().trim().to_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| index.to_string());

            Some(Contact {
                id,
                name,
                numbers,
                emails,
                organization,
                photo_base64,
            })
        })
        .collect()
}

/// Parse a call-history phonebook (`ich`, `och`, `mch`, `cch`).
///
/// Each vCard is one call. The kind comes from the IrMC property
/// `X-IRMC-CALL-DATETIME;MISSED|RECEIVED|DIALED:20261108T110455` (2.1 bare
/// parameter or 3.0 `TYPE=`). When the property has no kind (or is missing),
/// `default_kind` is used: pass the kind of the folder you downloaded (`ich` =
/// incoming, ...). Entries whose kind can't be determined at all (combined
/// `cch` without the parameter and no default) are skipped rather than guessed.
pub fn parse_call_history(
    data: &str,
    default_kind: Option<CallHistoryKind>,
) -> Vec<CallHistoryEntry> {
    parse_cards(data)
        .into_iter()
        .filter_map(|card| {
            let datetime = card.get("X-IRMC-CALL-DATETIME");
            let kind = datetime.and_then(call_kind).or(default_kind)?;
            let timestamp = datetime
                .map(|p| p.text().trim().to_owned())
                .filter(|t| !t.is_empty());
            let number = card
                .numbers()
                .into_iter()
                .next()
                .map(|n| n.number)
                .unwrap_or_default();
            let name = Some(card.display_name()).filter(|n| !n.is_empty());
            Some(CallHistoryEntry {
                kind,
                number,
                name,
                timestamp,
            })
        })
        .collect()
}

fn call_kind(p: &Property) -> Option<CallHistoryKind> {
    if p.has_flag("MISSED") {
        Some(CallHistoryKind::Missed)
    } else if p.has_flag("RECEIVED") {
        Some(CallHistoryKind::Incoming)
    } else if p.has_flag("DIALED") {
        Some(CallHistoryKind::Outgoing)
    } else {
        None
    }
}

/// `N` is `family;given;additional;prefix;suffix`. People expect
/// "prefix given additional family suffix", e.g. `Doe;John;;Dr.;` -> "Dr. John Doe".
fn format_structured_name(n: &Property) -> String {
    let c = n.components();
    let part = |i: usize| c.get(i).map(|s| s.trim()).unwrap_or("");
    [part(3), part(1), part(2), part(0), part(4)]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// First meaningful `TYPE` of a number. `voice` and `pref` are dropped because
/// nearly every number has them and they say nothing about which number it is.
fn number_kind(p: &Property) -> Option<String> {
    p.types()
        .into_iter()
        .find(|t| !matches!(t.as_str(), "voice" | "pref" | "internet" | "x-voice"))
}

fn clean_number(raw: &str) -> String {
    let n = raw.trim();
    let n = n
        .strip_prefix("tel:")
        .or_else(|| n.strip_prefix("TEL:"))
        .unwrap_or(n);
    n.trim().to_owned()
}

fn photo_base64(p: &Property) -> Option<String> {
    let encoding_b64 = p.param_values("ENCODING").any(|e| {
        let e = e.to_ascii_uppercase();
        e == "B" || e == "BASE64"
    });
    let data = if encoding_b64 {
        p.value.clone()
    } else if let Some((_, rest)) = p.value.split_once(";base64,") {
        // vCard 4 style `data:image/jpeg;base64,....` URI.
        rest.to_owned()
    } else {
        return None;
    };
    let cleaned: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    (!cleaned.is_empty()).then_some(cleaned)
}

// ---------------------------------------------------------------------------
// Low-level text handling
// ---------------------------------------------------------------------------

/// Join physical lines into logical lines (unfolding).
///
/// Rules, applied in this order for each physical line:
/// 1. Blank lines end any pending base64 block and are dropped.
/// 2. If the pending line is QUOTED-PRINTABLE and ends with `=`, that `=` is a
///    soft line break: drop it and append the next line verbatim.
/// 3. A line starting with space/tab continues the pending line. vCard 3.0
///    removes the one whitespace character; 2.1 (RFC 822 style) keeps it. We
///    track `VERSION` to know which rule applies.
/// 4. In a BASE64 property, a line with no `:` is more base64 (some 2.1
///    writers don't indent continuation lines).
fn logical_lines(data: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut is_v3 = false;
    let mut pending_is_qp = false;
    let mut pending_is_b64 = false;

    for raw in data.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);

        if let Some(last) = out.last_mut() {
            if pending_is_qp && last.ends_with('=') {
                last.pop();
                last.push_str(line);
                continue;
            }
            if line.starts_with([' ', '\t']) {
                if is_v3 {
                    last.push_str(&line[1..]);
                } else {
                    last.push_str(line);
                }
                continue;
            }
            if pending_is_b64 && !line.trim().is_empty() && !line.contains(':') {
                last.push_str(line.trim());
                continue;
            }
        }

        if line.trim().is_empty() {
            pending_is_b64 = false;
            pending_is_qp = false;
            continue;
        }

        // A new logical line starts here; classify it from its parameter part.
        let head = line.split(':').next().unwrap_or("").to_ascii_uppercase();
        if head == "VERSION" {
            is_v3 = !line.get(8..).unwrap_or("").trim().starts_with("2.");
        } else if head == "BEGIN" {
            // Each card states its own version; assume 2.1 rules until told otherwise.
            is_v3 = false;
        }
        pending_is_qp = head.contains("QUOTED-PRINTABLE");
        pending_is_b64 = head.contains("BASE64") || head.contains("ENCODING=B");
        out.push(line.to_owned());
    }
    out
}

/// Parse one logical line into a [`Property`]. Returns `None` for lines with no `:`.
fn parse_line(line: &str) -> Option<Property> {
    let colon = find_unquoted(line, ':')?;
    let (head, value) = (&line[..colon], &line[colon + 1..]);

    let mut parts = split_unquoted(head, ';').into_iter();
    let full_name = parts.next()?.trim().to_owned();
    if full_name.is_empty() {
        return None;
    }
    let (group, name) = match full_name.rsplit_once('.') {
        Some((g, n)) => (Some(g.to_owned()), n.to_ascii_uppercase()),
        None => (None, full_name.to_ascii_uppercase()),
    };

    let mut params = Vec::new();
    for p in parts {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        match p.split_once('=') {
            Some((k, v)) => {
                let k = k.trim().to_ascii_uppercase();
                for v in split_unquoted(v, ',') {
                    params.push((k.clone(), v.trim().trim_matches('"').to_owned()));
                }
            }
            None => {
                // vCard 2.1 bare parameter: an encoding or a type.
                let upper = p.to_ascii_uppercase();
                let key = match upper.as_str() {
                    "QUOTED-PRINTABLE" | "BASE64" | "B" | "8BIT" | "7BIT" => "ENCODING",
                    _ => "TYPE",
                };
                params.push((key.to_owned(), p.to_owned()));
            }
        }
    }

    let mut prop = Property {
        group,
        name,
        params,
        value: String::new(),
    };
    let is_qp = prop
        .param_values("ENCODING")
        .any(|e| e.eq_ignore_ascii_case("QUOTED-PRINTABLE"));
    prop.value = if is_qp {
        let bytes = decode_quoted_printable(value);
        let charset = prop
            .param_values("CHARSET")
            .next()
            .unwrap_or("UTF-8")
            .to_ascii_uppercase();
        decode_charset(&bytes, &charset)
    } else {
        value.to_owned()
    };
    Some(prop)
}

/// Decode QUOTED-PRINTABLE text to raw bytes. `=XX` becomes the byte `0xXX`;
/// a stray `=` that isn't followed by two hex digits is kept literally.
pub fn decode_quoted_printable(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Turn decoded bytes into text. UTF-8 is the norm on modern phones; Latin-1
/// family charsets are mapped byte-for-byte; anything unknown is read as
/// UTF-8 with replacement characters rather than failing.
fn decode_charset(bytes: &[u8], charset: &str) -> String {
    match charset {
        "ISO-8859-1" | "ISO8859-1" | "LATIN1" | "WINDOWS-1252" | "CP1252" | "US-ASCII" => {
            bytes.iter().map(|&b| b as char).collect()
        }
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Remove vCard escapes: `\n` / `\N` -> newline, `\,` `\;` `\:` `\\` -> the character.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') | Some('N') => out.push('\n'),
            Some(other @ (',' | ';' | ':' | '\\')) => out.push(other),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Split on `sep` unless preceded by a backslash (escapes are kept for [`unescape`]).
fn split_unescaped(s: &str, sep: char) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut escaped = false;
    for c in s.chars() {
        let cur = parts.last_mut().expect("parts is never empty");
        if escaped {
            cur.push(c);
            escaped = false;
        } else if c == '\\' {
            cur.push(c);
            escaped = true;
        } else if c == sep {
            parts.push(String::new());
        } else {
            cur.push(c);
        }
    }
    parts
}

/// Split on `sep` outside double quotes.
fn split_unquoted(s: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    for (i, c) in s.char_indices() {
        if c == '"' {
            quoted = !quoted;
        } else if c == sep && !quoted {
            parts.push(&s[start..i]);
            start = i + c.len_utf8();
        }
    }
    parts.push(&s[start..]);
    parts
}

fn find_unquoted(s: &str, needle: char) -> Option<usize> {
    let mut quoted = false;
    for (i, c) in s.char_indices() {
        if c == '"' {
            quoted = !quoted;
        } else if c == needle && !quoted {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crlf(s: &str) -> String {
        s.replace('\n', "\r\n")
    }

    #[test]
    fn vcard30_basic_contact() {
        let data = crlf(
            "BEGIN:VCARD\nVERSION:3.0\nFN:John Doe\nN:Doe;John;;;\nTEL;TYPE=CELL,VOICE:+1 555 0100\n\
             TEL;TYPE=WORK:555-0199\nEMAIL;TYPE=INTERNET:john@example.com\nORG:Acme Inc.;R&D\nEND:VCARD\n",
        );
        let contacts = parse_contacts(&data);
        assert_eq!(contacts.len(), 1);
        let c = &contacts[0];
        assert_eq!(c.id, "0");
        assert_eq!(c.name, "John Doe");
        assert_eq!(
            c.numbers,
            vec![
                PhoneNumber {
                    number: "+1 555 0100".into(),
                    kind: Some("cell".into())
                },
                PhoneNumber {
                    number: "555-0199".into(),
                    kind: Some("work".into())
                },
            ]
        );
        assert_eq!(c.emails, vec!["john@example.com".to_string()]);
        assert_eq!(c.organization.as_deref(), Some("Acme Inc., R&D"));
        assert_eq!(c.photo_base64, None);
    }

    #[test]
    fn vcard21_bare_params_and_repeated_type() {
        let data = "BEGIN:VCARD\nVERSION:2.1\nN:Smith;Anna\nTEL;CELL;VOICE:0711\nTEL;HOME:0722\n\
                    TEL;VOICE:0733\nTEL;TYPE=PREF;TYPE=HOME:0744\nEND:VCARD\n";
        let c = &parse_contacts(data)[0];
        assert_eq!(c.name, "Anna Smith");
        let kinds: Vec<_> = c.numbers.iter().map(|n| n.kind.as_deref()).collect();
        assert_eq!(kinds, vec![Some("cell"), Some("home"), None, Some("home")]);
    }

    #[test]
    fn n_fallback_with_prefix_suffix_and_empty_fn() {
        let data = "BEGIN:VCARD\nVERSION:3.0\nFN:\nN:Doe;John;Q;Dr.;Jr.\nTEL:1\nEND:VCARD";
        assert_eq!(parse_contacts(data)[0].name, "Dr. John Q Doe Jr.");
    }

    #[test]
    fn name_falls_back_to_org_then_number() {
        let data = "BEGIN:VCARD\nVERSION:3.0\nN:;;;;\nORG:Pizza Place;\nTEL:123\nEND:VCARD\n\
                    BEGIN:VCARD\nVERSION:3.0\nN:;;;;\nTEL:456\nEND:VCARD\n";
        let cs = parse_contacts(data);
        assert_eq!(cs[0].name, "Pizza Place");
        assert_eq!(cs[0].organization.as_deref(), Some("Pizza Place"));
        assert_eq!(cs[1].name, "456");
        assert_eq!(cs[1].id, "1");
    }

    #[test]
    fn empty_owner_card_is_dropped_but_ids_keep_positions() {
        let data = "BEGIN:VCARD\nVERSION:2.1\nN:\nTEL:\nEND:VCARD\nBEGIN:VCARD\nVERSION:2.1\nFN:Bob\nTEL:9\nEND:VCARD\n";
        let cs = parse_contacts(data);
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].name, "Bob");
        assert_eq!(cs[0].id, "1");
    }

    #[test]
    fn uid_is_preferred_as_id() {
        let data = "BEGIN:VCARD\nVERSION:3.0\nFN:A\nUID:abc-123\nEND:VCARD";
        assert_eq!(parse_contacts(data)[0].id, "abc-123");
    }

    #[test]
    fn vcard21_quoted_printable_utf8_with_soft_breaks() {
        // "José Müller-Lüdenscheidt" split by a QP soft line break.
        let data = crlf(
            "BEGIN:VCARD\nVERSION:2.1\nN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:M=C3=BCller-L=C3=BC=\ndenscheidt;Jos=C3=A9;;;\n\
             FN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:Jos=C3=A9 M=C3=BCller-L=C3=BC=\ndenscheidt\nTEL;CELL:+49 1\nEND:VCARD\n",
        );
        let c = &parse_contacts(&data)[0];
        assert_eq!(c.name, "José Müller-Lüdenscheidt");
        let cards = parse_cards(&data);
        assert_eq!(cards[0].display_name(), "José Müller-Lüdenscheidt");
        assert_eq!(
            format_structured_name(cards[0].get("N").unwrap()),
            "José Müller-Lüdenscheidt"
        );
    }

    #[test]
    fn vcard21_bare_quoted_printable_and_latin1() {
        let data = "BEGIN:VCARD\nVERSION:2.1\nFN;QUOTED-PRINTABLE;CHARSET=ISO-8859-1:Ren=E9\nTEL:1\nEND:VCARD";
        assert_eq!(parse_contacts(data)[0].name, "René");
    }

    #[test]
    fn qp_stray_equals_kept() {
        assert_eq!(decode_quoted_printable("a=zz=4"), b"a=zz=4".to_vec());
        assert_eq!(decode_quoted_printable("=41=42"), b"AB".to_vec());
        assert_eq!(decode_quoted_printable("x=4"), b"x=4".to_vec());
    }

    #[test]
    fn folded_lines_v30_and_v21() {
        // 3.0: the single leading space is removed.
        let v3 = crlf("BEGIN:VCARD\nVERSION:3.0\nFN:Alexandra Very\n Long Name\nEND:VCARD\n");
        assert_eq!(
            parse_cards(&v3)[0].display_name(),
            "Alexandra VeryLong Name"
        );
        // 2.1: the whitespace is kept (RFC 822 folding).
        let v21 = crlf("BEGIN:VCARD\nVERSION:2.1\nFN:Alexandra Very\n Long Name\nEND:VCARD\n");
        assert_eq!(
            parse_cards(&v21)[0].display_name(),
            "Alexandra Very Long Name"
        );
    }

    #[test]
    fn escapes_are_removed() {
        let data =
            "BEGIN:VCARD\nVERSION:3.0\nFN:Doe\\, John\nORG:A\\;B;C\nNOTE:line1\\nline2\nEND:VCARD";
        let cards = parse_cards(data);
        assert_eq!(cards[0].display_name(), "Doe, John");
        assert_eq!(cards[0].get("ORG").unwrap().components(), vec!["A;B", "C"]);
        assert_eq!(cards[0].get("NOTE").unwrap().text(), "line1\nline2");
        assert_eq!(
            parse_contacts(data)[0].organization.as_deref(),
            Some("A;B, C")
        );
    }

    #[test]
    fn photo_base64_v30_folded() {
        let data = crlf(
            "BEGIN:VCARD\nVERSION:3.0\nFN:Pic\nPHOTO;ENCODING=b;TYPE=JPEG:/9j/4AAQ\n SkZJRgAB\n AQAAAQ==\nTEL:1\nEND:VCARD\n",
        );
        let c = &parse_contacts(&data)[0];
        assert_eq!(c.photo_base64.as_deref(), Some("/9j/4AAQSkZJRgABAQAAAQ=="));
        assert_eq!(c.numbers.len(), 1);
    }

    #[test]
    fn photo_base64_v21_unindented_with_blank_line() {
        let data = crlf(
            "BEGIN:VCARD\nVERSION:2.1\nFN:Pic\nPHOTO;ENCODING=BASE64;TYPE=JPEG:/9j/4AAQ\nSkZJRgAB\n    AQAAAQ==\n\nTEL;CELL:1\nEND:VCARD\n",
        );
        let c = &parse_contacts(&data)[0];
        assert_eq!(c.photo_base64.as_deref(), Some("/9j/4AAQSkZJRgABAQAAAQ=="));
        assert_eq!(c.numbers[0].kind.as_deref(), Some("cell"));
    }

    #[test]
    fn photo_uri_without_data_is_ignored() {
        let data =
            "BEGIN:VCARD\nVERSION:3.0\nFN:P\nPHOTO;VALUE=uri:http://example.com/a.jpg\nEND:VCARD";
        assert_eq!(parse_contacts(data)[0].photo_base64, None);
    }

    #[test]
    fn apple_grouped_properties_and_tel_uri() {
        let data = "BEGIN:VCARD\nVERSION:3.0\nFN:G\nitem1.TEL;type=pref:tel:+33 1\nitem1.X-ABLabel:Other\nEND:VCARD";
        let cards = parse_cards(data);
        let tel = cards[0].get("TEL").unwrap();
        assert_eq!(tel.group.as_deref(), Some("item1"));
        let c = &parse_contacts(data)[0];
        assert_eq!(
            c.numbers,
            vec![PhoneNumber {
                number: "+33 1".into(),
                kind: None
            }]
        );
    }

    #[test]
    fn quoted_param_values_may_contain_separators() {
        let p = parse_line("X-TEST;LABEL=\"a;b:c\":value").unwrap();
        assert_eq!(p.param_values("LABEL").collect::<Vec<_>>(), vec!["a;b:c"]);
        assert_eq!(p.value, "value");
    }

    #[test]
    fn nested_agent_card_is_skipped_and_truncated_card_kept() {
        let data = "BEGIN:VCARD\nVERSION:2.1\nFN:Boss\nAGENT:\nBEGIN:VCARD\nVERSION:2.1\nFN:Assistant\nEND:VCARD\n\
                    TEL:1\nEND:VCARD\nBEGIN:VCARD\nVERSION:2.1\nFN:Cut";
        let cs = parse_contacts(data);
        assert_eq!(cs.len(), 2);
        assert_eq!(cs[0].name, "Boss");
        assert_eq!(cs[0].numbers.len(), 1);
        assert_eq!(cs[1].name, "Cut");
    }

    #[test]
    fn garbage_is_ignored() {
        assert!(parse_contacts("").is_empty());
        assert!(parse_contacts("hello\nworld").is_empty());
        let data = "junk\nBEGIN:VCARD\nno colon line\nFN:Ok\nEND:VCARD\ntrailing";
        assert_eq!(parse_contacts(data)[0].name, "Ok");
    }

    #[test]
    fn call_history_v21_bare_kinds() {
        let data = crlf(
            "BEGIN:VCARD\nVERSION:2.1\nN:Doe;Jane\nTEL;CELL:+15550100\nX-IRMC-CALL-DATETIME;MISSED:20261108T110455\nEND:VCARD\n\
             BEGIN:VCARD\nVERSION:2.1\nN:\nTEL:+15550111\nX-IRMC-CALL-DATETIME;RECEIVED:20261107T090000\nEND:VCARD\n\
             BEGIN:VCARD\nVERSION:2.1\nFN:Bob\nTEL:+15550122\nX-IRMC-CALL-DATETIME;DIALED:20261106T080000Z\nEND:VCARD\n",
        );
        let h = parse_call_history(&data, None);
        assert_eq!(
            h,
            vec![
                CallHistoryEntry {
                    kind: CallHistoryKind::Missed,
                    number: "+15550100".into(),
                    name: Some("Jane Doe".into()),
                    timestamp: Some("20261108T110455".into()),
                },
                CallHistoryEntry {
                    kind: CallHistoryKind::Incoming,
                    number: "+15550111".into(),
                    name: None,
                    timestamp: Some("20261107T090000".into()),
                },
                CallHistoryEntry {
                    kind: CallHistoryKind::Outgoing,
                    number: "+15550122".into(),
                    name: Some("Bob".into()),
                    timestamp: Some("20261106T080000Z".into()),
                },
            ]
        );
    }

    #[test]
    fn call_history_v30_type_param() {
        let data = "BEGIN:VCARD\nVERSION:3.0\nFN:\nN:;;;;\nTEL:\nX-IRMC-CALL-DATETIME;TYPE=MISSED:20260101T000000\nEND:VCARD";
        let h = parse_call_history(data, None);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].kind, CallHistoryKind::Missed);
        assert_eq!(h[0].number, "");
        assert_eq!(h[0].name, None);
    }

    #[test]
    fn call_history_default_kind_and_skip_unknown() {
        let data = "BEGIN:VCARD\nVERSION:3.0\nTEL:1\nEND:VCARD\n\
                    BEGIN:VCARD\nVERSION:3.0\nTEL:2\nX-IRMC-CALL-DATETIME:20260101T000000\nEND:VCARD";
        let with_default = parse_call_history(data, Some(CallHistoryKind::Outgoing));
        assert_eq!(with_default.len(), 2);
        assert!(with_default
            .iter()
            .all(|e| e.kind == CallHistoryKind::Outgoing));
        assert_eq!(with_default[0].timestamp, None);
        assert_eq!(
            with_default[1].timestamp.as_deref(),
            Some("20260101T000000")
        );
        assert!(parse_call_history(data, None).is_empty());
        // The property's own kind wins over the default.
        let missed = "BEGIN:VCARD\nVERSION:3.0\nTEL:1\nX-IRMC-CALL-DATETIME;MISSED:2026\nEND:VCARD";
        assert_eq!(
            parse_call_history(missed, Some(CallHistoryKind::Incoming))[0].kind,
            CallHistoryKind::Missed
        );
    }

    #[test]
    fn lf_only_and_lowercase_keywords() {
        let data = "begin:vcard\nversion:3.0\nfn:Lower\ntel;type=home:1\nend:vcard\n";
        let c = &parse_contacts(data)[0];
        assert_eq!(c.name, "Lower");
        assert_eq!(c.numbers[0].kind.as_deref(), Some("home"));
    }
}
