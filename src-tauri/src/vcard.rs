// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! vCard, so contacts move to and from Outlook, Google, Apple, Thunderbird
//! and Nextcloud.
//!
//! Read: 2.1 (old Outlook, including QUOTED-PRINTABLE), 3.0 (Google, Apple)
//! and 4.0 (RFC 6350); folded lines, escaped text, grouped properties
//! (`item1.EMAIL`), unknown properties ignored. Written: 3.0, which every one
//! of those reads.
//!
//! A contact here: `{ name, given, family, org, title, emails: [{type,value}],
//! phones: [{type,value}], addresses: [{type,street,city,region,postcode,
//! country}], birthday, url, note, groups: [] }`.

use serde_json::{json, Value};

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n' | 'N') => out.push('\n'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Split on unescaped `;` (structured values: N, ADR, ORG).
fn components(value: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut escaped = false;
    for c in value.chars() {
        if escaped {
            if let Some(s) = out.last_mut() {
                s.push('\\');
                s.push(c);
            }
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == ';' {
            out.push(String::new());
        } else if let Some(s) = out.last_mut() {
            s.push(c);
        }
    }
    out.into_iter().map(|s| unescape(&s)).collect()
}

fn quoted_printable(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' && i + 2 < bytes.len() + 1 {
            if let Some(hex) = text.get(i + 1..i + 3) {
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    out.push(b);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn fold(line: &str, out: &mut String) {
    let mut width = 0;
    for c in line.chars() {
        if width + c.len_utf8() > 75 {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(c);
        width += c.len_utf8();
    }
    out.push_str("\r\n");
}

/// The `TYPE` of a property, lowercased, first useful one: `work`, `home`,
/// `cell`... `pref` and `internet` say nothing about which address it is.
fn kind(params: &[(String, String)]) -> String {
    for (name, value) in params {
        let values: Vec<String> = if name == "TYPE" {
            value
                .split(',')
                .map(|v| v.trim_matches('"').to_ascii_lowercase())
                .collect()
        } else if value.is_empty() {
            // vCard 2.1: `TEL;WORK;VOICE:...`
            vec![name.to_ascii_lowercase()]
        } else {
            continue;
        };
        if let Some(v) = values
            .into_iter()
            .find(|v| !matches!(v.as_str(), "pref" | "internet" | "voice" | "x400"))
        {
            return v;
        }
    }
    String::new()
}

pub fn export(contacts: &[Value]) -> String {
    let mut out = String::new();
    for c in contacts {
        fold("BEGIN:VCARD", &mut out);
        fold("VERSION:3.0", &mut out);
        if let Some(id) = c["id"].as_str() {
            fold(&format!("UID:{id}@eon-office-contacts"), &mut out);
        }
        let s = |k: &str| c[k].as_str().unwrap_or_default().to_owned();
        fold(&format!("FN:{}", escape(&s("name"))), &mut out);
        fold(
            &format!("N:{};{};;;", escape(&s("family")), escape(&s("given"))),
            &mut out,
        );
        if !s("org").is_empty() {
            fold(&format!("ORG:{}", escape(&s("org"))), &mut out);
        }
        if !s("title").is_empty() {
            fold(&format!("TITLE:{}", escape(&s("title"))), &mut out);
        }
        for (prop, key) in [("EMAIL", "emails"), ("TEL", "phones")] {
            for item in c[key].as_array().into_iter().flatten() {
                let t = item["type"].as_str().unwrap_or_default();
                let param = if t.is_empty() {
                    String::new()
                } else {
                    format!(";TYPE={}", t.to_ascii_uppercase())
                };
                fold(
                    &format!(
                        "{prop}{param}:{}",
                        escape(item["value"].as_str().unwrap_or_default())
                    ),
                    &mut out,
                );
            }
        }
        for a in c["addresses"].as_array().into_iter().flatten() {
            let t = a["type"].as_str().unwrap_or_default();
            let param = if t.is_empty() {
                String::new()
            } else {
                format!(";TYPE={}", t.to_ascii_uppercase())
            };
            let part = |k: &str| escape(a[k].as_str().unwrap_or_default());
            fold(
                &format!(
                    "ADR{param}:;;{};{};{};{};{}",
                    part("street"),
                    part("city"),
                    part("region"),
                    part("postcode"),
                    part("country")
                ),
                &mut out,
            );
        }
        if !s("birthday").is_empty() {
            fold(&format!("BDAY:{}", s("birthday")), &mut out);
        }
        if !s("url").is_empty() {
            fold(&format!("URL:{}", escape(&s("url"))), &mut out);
        }
        if !s("note").is_empty() {
            fold(&format!("NOTE:{}", escape(&s("note"))), &mut out);
        }
        let groups: Vec<String> = c["groups"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|g| g.as_str().map(escape))
            .collect();
        if !groups.is_empty() {
            fold(&format!("CATEGORIES:{}", groups.join(",")), &mut out);
        }
        fold("END:VCARD", &mut out);
    }
    out
}

fn birthday(value: &str) -> String {
    let digits: String = value.chars().filter(char::is_ascii_digit).collect();
    if digits.len() >= 8 && !value.starts_with("--") {
        format!("{}-{}-{}", &digits[..4], &digits[4..6], &digits[6..8])
    } else {
        String::new()
    }
}

#[allow(clippy::too_many_lines)] // one match over the properties reads best whole
pub fn import(text: &str) -> Vec<Value> {
    let mut lines: Vec<String> = Vec::new();
    let mut soft_break = false;
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        // QUOTED-PRINTABLE soft line breaks end in `=`.
        if soft_break {
            if let Some(last) = lines.last_mut() {
                last.pop();
                last.push_str(raw);
                soft_break = raw.ends_with('=');
                continue;
            }
        }
        if let Some(rest) = raw.strip_prefix(' ').or_else(|| raw.strip_prefix('\t')) {
            if let Some(last) = lines.last_mut() {
                last.push_str(rest);
                continue;
            }
        }
        soft_break = raw.to_ascii_uppercase().contains("QUOTED-PRINTABLE") && raw.ends_with('=');
        lines.push(raw.to_owned());
    }
    let mut out = Vec::new();
    let mut card: Option<Value> = None;
    for line in lines {
        let Some((head, value)) = line.split_once(':') else {
            continue;
        };
        let mut parts = head.split(';');
        let name = parts.next().unwrap_or_default();
        let name = name.rsplit('.').next().unwrap_or(name).to_ascii_uppercase();
        let params: Vec<(String, String)> = parts
            .map(|p| match p.split_once('=') {
                Some((k, v)) => (k.to_ascii_uppercase(), v.to_owned()),
                None => (p.to_ascii_uppercase(), String::new()),
            })
            .collect();
        let qp = params.iter().any(|(k, v)| {
            (k == "ENCODING" && v.eq_ignore_ascii_case("QUOTED-PRINTABLE"))
                || k == "QUOTED-PRINTABLE"
        });
        let value = if qp {
            quoted_printable(value)
        } else {
            value.to_owned()
        };
        match (name.as_str(), card.as_mut()) {
            ("BEGIN", None) if value.eq_ignore_ascii_case("VCARD") => {
                card = Some(json!({ "emails": [], "phones": [], "addresses": [], "groups": [] }));
            }
            ("END", Some(_)) if value.eq_ignore_ascii_case("VCARD") => {
                if let Some(mut c) = card.take() {
                    if c["name"].as_str().is_none_or(str::is_empty) {
                        let joined = [c["given"].as_str(), c["family"].as_str()]
                            .into_iter()
                            .flatten()
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                            .join(" ");
                        let fallback = if joined.is_empty() {
                            c["emails"][0]["value"]
                                .as_str()
                                .unwrap_or_default()
                                .to_owned()
                        } else {
                            joined
                        };
                        c["name"] = json!(fallback);
                    }
                    if c["name"].as_str().is_some_and(|n| !n.is_empty()) {
                        out.push(c);
                    }
                }
            }
            ("UID", Some(c)) => c["uid"] = json!(value.trim_end_matches("@eon-office-contacts")),
            ("FN", Some(c)) => c["name"] = json!(unescape(&value).trim()),
            ("N", Some(c)) => {
                let n = components(&value);
                c["family"] = json!(n.first().cloned().unwrap_or_default());
                c["given"] = json!(n.get(1).cloned().unwrap_or_default());
            }
            ("ORG", Some(c)) => {
                c["org"] = json!(components(&value)
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(", "));
            }
            ("TITLE", Some(c)) => c["title"] = json!(unescape(&value)),
            ("EMAIL", Some(c)) => {
                if let Some(list) = c["emails"].as_array_mut() {
                    list.push(json!({ "type": kind(&params), "value": unescape(&value).trim() }));
                }
            }
            ("TEL", Some(c)) => {
                let number = unescape(&value);
                let number = number.trim().trim_start_matches("tel:");
                if let Some(list) = c["phones"].as_array_mut() {
                    list.push(json!({ "type": kind(&params), "value": number }));
                }
            }
            ("ADR", Some(c)) => {
                let a = components(&value);
                let at = |i: usize| a.get(i).cloned().unwrap_or_default();
                if let Some(list) = c["addresses"].as_array_mut() {
                    list.push(json!({ "type": kind(&params), "street": at(2), "city": at(3), "region": at(4), "postcode": at(5), "country": at(6) }));
                }
            }
            ("BDAY", Some(c)) => c["birthday"] = json!(birthday(&value)),
            ("URL", Some(c)) => c["url"] = json!(unescape(&value)),
            ("NOTE", Some(c)) => c["note"] = json!(unescape(&value)),
            ("CATEGORIES", Some(c)) => {
                let groups: Vec<Value> = value
                    .split(',')
                    .map(|g| json!(unescape(g).trim()))
                    .filter(|g| g.as_str().is_some_and(|s| !s.is_empty()))
                    .collect();
                c["groups"] = json!(groups);
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_contact_survives_a_round_trip() {
        let c = json!({
            "id": "c1", "name": "Ayşe Yılmaz", "given": "Ayşe", "family": "Yılmaz", "org": "Acme; Ltd",
            "title": "Engineer", "emails": [{ "type": "work", "value": "ayse@acme.example" }],
            "phones": [{ "type": "cell", "value": "+90 555 000 00 00" }],
            "addresses": [{ "type": "work", "street": "İstiklal Cd. 1", "city": "İstanbul", "region": "", "postcode": "34430", "country": "Türkiye" }],
            "birthday": "1990-04-23", "note": "Met at the conference,\nloves tea", "groups": ["Work", "Friends"],
        });
        let text = export(&[c]);
        assert!(text.lines().all(|l| l.len() <= 76));
        let back = &import(&text)[0];
        for key in [
            "name", "given", "family", "org", "title", "birthday", "note",
        ] {
            assert_eq!(
                back[key],
                json!({
                    "name": "Ayşe Yılmaz", "given": "Ayşe", "family": "Yılmaz", "org": "Acme; Ltd", "title": "Engineer",
                    "birthday": "1990-04-23", "note": "Met at the conference,\nloves tea",
                })[key],
                "{key}"
            );
        }
        assert_eq!(
            back["emails"][0],
            json!({ "type": "work", "value": "ayse@acme.example" })
        );
        assert_eq!(back["phones"][0]["type"], "cell");
        assert_eq!(back["addresses"][0]["city"], "İstanbul");
        assert_eq!(back["groups"], json!(["Work", "Friends"]));
    }

    #[test]
    fn google_and_apple_style_3_0_with_groups_and_folding() {
        let text = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Grace Hopper\r\nN:Hopper;Grace;;;\r\nitem1.EMAIL;TYPE=INTERNET,PREF:grace@navy.example\r\nitem1.X-ABLabel:Navy\r\nTEL;TYPE=CELL:+1 555\r\n 0100\r\nBDAY:1906-12-09\r\nEND:VCARD\r\n";
        let c = &import(text)[0];
        assert_eq!(c["emails"][0]["value"], "grace@navy.example");
        assert_eq!(
            c["phones"][0],
            json!({ "type": "cell", "value": "+1 5550100" })
        );
        assert_eq!(c["birthday"], "1906-12-09");
    }

    #[test]
    fn rfc_6350_4_0_and_old_outlook_2_1() {
        let v4 = "BEGIN:VCARD\nVERSION:4.0\nFN:Ada Lovelace\nTEL;VALUE=uri;TYPE=home:tel:+44-20-0000\nEMAIL;TYPE=work:ada@example.org\nBDAY:--1210\nEND:VCARD\n";
        let c = &import(v4)[0];
        assert_eq!(
            c["phones"][0],
            json!({ "type": "home", "value": "+44-20-0000" })
        );
        assert_eq!(
            c["birthday"], "",
            "a birthday without a year is not invented"
        );
        let v21 = "BEGIN:VCARD\r\nVERSION:2.1\r\nN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:Y=C4=B1lmaz;Ay=C5=9Fe\r\nTEL;WORK;VOICE:+90 212 000 00 00\r\nNOTE;ENCODING=QUOTED-PRINTABLE:Bir=\r\n iki\r\nEND:VCARD\r\n";
        let c = &import(v21)[0];
        assert_eq!(
            (c["given"].clone(), c["family"].clone(), c["name"].clone()),
            (json!("Ayşe"), json!("Yılmaz"), json!("Ayşe Yılmaz"))
        );
        assert_eq!(c["phones"][0]["type"], "work");
    }

    #[test]
    fn a_card_without_any_name_takes_its_email() {
        let c = &import("BEGIN:VCARD\nVERSION:3.0\nEMAIL:x@example.org\nEND:VCARD\n")[0];
        assert_eq!(c["name"], "x@example.org");
        assert!(import("BEGIN:VCARD\nVERSION:3.0\nEND:VCARD\n").is_empty());
    }
}
