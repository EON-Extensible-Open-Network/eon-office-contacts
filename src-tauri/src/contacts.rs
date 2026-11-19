// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! Contacts, kept in the EON Office core, and found for other applications.
//!
//! Lists sort by name the way a Turkish reader expects (`Çağla` after
//! `Cem`, `İrem` with the i's), using the same folding as search. Other
//! applications ask `contact.lookup` with an e-mail address or part of a
//! name and get back names and addresses -- never the whole card -- and only
//! if the person allowed them `call:org.eon.office.contacts`.

use crate::vcard;
use eon_office_sdk::{Client, Error};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};

pub const COLLECTION: &str = "contacts";
pub type Outcome = Result<Value, Value>;

fn fail(code: &str, message: impl Into<String>) -> Value {
    json!({ "code": code, "message": message.into() })
}

fn sdk<T>(result: Result<T, Error>) -> Result<T, Value> {
    result.map_err(|e| {
        let mut out = fail(e.code(), e.to_string());
        if let Error::Protocol { data, .. } = &e {
            out["data"] = data.clone();
        }
        out
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0))
}

static COUNTER: AtomicU64 = AtomicU64::new(0);
fn new_id() -> String {
    format!(
        "c{:x}{:x}",
        now_ms(),
        COUNTER.fetch_add(1, Ordering::Relaxed) % 4096
    )
}

/// Sort key: the Turkish alphabet's order (`c < ç < d`, `g < ğ < h`, `ı < i`,
/// `o < ö < p`, `s < ş < t`, `u < ü < v`), with `q`, `w`, `x` where Latin puts
/// them, and case folded the Turkish way (`I` is `ı`, `İ` is `i`). Letters map
/// to a private range in that order; anything else keeps its own place.
pub fn sort_key(text: &str) -> String {
    const ALPHABET: &str = "abcçdefgğhıijklmnoöpqrsştuüvwxyz";
    text.chars()
        .map(|c| {
            let lower = match c {
                'I' => 'ı',
                'İ' => 'i',
                other => other.to_lowercase().next().unwrap_or(other),
            };
            ALPHABET
                .chars()
                .position(|a| a == lower)
                .and_then(|i| char::from_u32(0xE000 + u32::try_from(i).unwrap_or(0)))
                .unwrap_or(lower)
        })
        .collect()
}

/// Folding for matching: every i alike, diacritics dropped.
pub fn fold(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'I' | 'İ' | 'ı' | 'i' => 'i',
            'Ş' | 'ş' => 's',
            'Ğ' | 'ğ' => 'g',
            'Ç' | 'ç' => 'c',
            'Ö' | 'ö' => 'o',
            'Ü' | 'ü' => 'u',
            other => other.to_lowercase().next().unwrap_or(other),
        })
        .collect()
}

fn clean(fields: &Value) -> Result<Value, Value> {
    let name = fields["name"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_owned();
    let given = fields["given"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_owned();
    let family = fields["family"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_owned();
    let name = if name.is_empty() {
        [given.as_str(), family.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        name
    };
    if name.is_empty() || name.chars().count() > 200 {
        return Err(fail(
            "invalid_params",
            "a contact needs a name, at most 200 characters",
        ));
    }
    let list = |key: &str| -> Vec<Value> {
        fields[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|i| i["value"].as_str().is_some_and(|v| !v.trim().is_empty()))
            .map(|i| json!({ "type": i["type"].as_str().unwrap_or_default(), "value": i["value"].as_str().unwrap_or_default().trim() }))
            .collect()
    };
    let emails = list("emails");
    if let Some(bad) = emails.iter().find(|e| {
        !e["value"]
            .as_str()
            .is_some_and(|v| v.contains('@') && !v.contains(' '))
    }) {
        return Err(fail(
            "invalid_params",
            format!(
                "`{}` is not an e-mail address",
                bad["value"].as_str().unwrap_or_default()
            ),
        ));
    }
    let birthday = fields["birthday"].as_str().unwrap_or_default();
    let ok_date = birthday.is_empty()
        || (birthday.len() == 10
            && birthday.bytes().enumerate().all(|(i, b)| {
                if i == 4 || i == 7 {
                    b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            }));
    if !ok_date {
        return Err(fail("invalid_params", "a birthday is YYYY-MM-DD"));
    }
    let groups: BTreeSet<String> = fields["groups"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| g.as_str())
        .map(|g| g.trim().to_owned())
        .filter(|g| !g.is_empty())
        .collect();
    Ok(json!({
        "name": name, "given": given, "family": family,
        "org": fields["org"].as_str().unwrap_or_default().trim(),
        "title": fields["title"].as_str().unwrap_or_default().trim(),
        "emails": emails, "phones": list("phones"),
        "addresses": fields["addresses"].as_array().cloned().unwrap_or_default(),
        "birthday": birthday, "url": fields["url"].as_str().unwrap_or_default().trim(),
        "note": fields["note"].as_str().unwrap_or_default(),
        "groups": groups.into_iter().collect::<Vec<_>>(),
    }))
}

pub struct Contacts {
    pub client: Client,
}

impl Contacts {
    pub fn new(client: Client) -> Contacts {
        Contacts { client }
    }

    fn all(&self) -> Result<Vec<Value>, Value> {
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let mut params = json!({ "collection": COLLECTION, "limit": 1000 });
            if let Some(a) = &after {
                params["after"] = json!(a);
            }
            let page = sdk(self.client.call("storage.list", params))?;
            let items = page["items"].as_array().cloned().unwrap_or_default();
            for item in &items {
                let mut c = item["doc"].clone();
                c["id"] = item["id"].clone();
                c["rev"] = item["rev"].clone();
                out.push(c);
            }
            if items.len() < 1000 {
                break;
            }
            after = items
                .last()
                .and_then(|i| i["id"].as_str())
                .map(str::to_owned);
        }
        Ok(out)
    }

    /// Everyone (or one group), sorted by name; summary fields only.
    pub fn list(&self, group: Option<&str>) -> Outcome {
        let all = self.all()?;
        let mut groups: BTreeMap<String, usize> = BTreeMap::new();
        for c in &all {
            for g in c["groups"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                *groups.entry(g.to_owned()).or_default() += 1;
            }
        }
        let mut people: Vec<Value> = all
            .into_iter()
            .filter(|c| {
                group.is_none_or(|g| {
                    c["groups"]
                        .as_array()
                        .is_some_and(|gs| gs.iter().any(|x| x == g))
                })
            })
            .map(|c| {
                json!({
                    "id": c["id"], "name": c["name"], "org": c["org"],
                    "email": c["emails"][0]["value"], "phone": c["phones"][0]["value"],
                })
            })
            .collect();
        people.sort_by_key(|c| sort_key(c["name"].as_str().unwrap_or_default()));
        Ok(json!({ "contacts": people, "groups": groups }))
    }

    pub fn get(&self, id: &str) -> Outcome {
        let found = sdk(self.client.get(COLLECTION, id))?
            .ok_or_else(|| fail("not_found", "no such contact"))?;
        let mut c = found["doc"].clone();
        c["id"] = json!(id);
        c["rev"] = found["rev"].clone();
        let _ = self.client.publish(
            "activity",
            &json!({ "id": id, "title": c["name"], "kind": "opened" }),
        );
        Ok(c)
    }

    fn write(&self, id: &str, contact: &Value, rev: Option<u64>) -> Outcome {
        let written = sdk(self.client.put(COLLECTION, id, contact, rev))?;
        let mut text: Vec<String> = Vec::new();
        for key in ["org", "title", "note"] {
            text.push(contact[key].as_str().unwrap_or_default().to_owned());
        }
        for key in ["emails", "phones"] {
            for item in contact[key].as_array().into_iter().flatten() {
                text.push(item["value"].as_str().unwrap_or_default().to_owned());
            }
        }
        let _ = self.client.index(
            id,
            contact["name"].as_str().unwrap_or_default(),
            &text.join(" "),
            false,
        );
        Ok(json!({ "id": id, "rev": written["rev"] }))
    }

    pub fn create(&self, fields: &Value) -> Outcome {
        let mut contact = clean(fields)?;
        contact["created"] = json!(now_ms());
        contact["updated"] = json!(now_ms());
        let id = new_id();
        let out = self.write(&id, &contact, Some(0))?;
        let _ = self.client.publish(
            "activity",
            &json!({ "id": id, "title": contact["name"], "kind": "created" }),
        );
        Ok(out)
    }

    pub fn update(&self, id: &str, fields: &Value, rev: u64) -> Outcome {
        let existing = sdk(self.client.get(COLLECTION, id))?
            .ok_or_else(|| fail("not_found", "no such contact"))?;
        let mut contact = clean(fields)?;
        contact["created"] = existing["doc"]["created"].clone();
        contact["updated"] = json!(now_ms());
        let out = self.write(id, &contact, Some(rev))?;
        let _ = self.client.publish(
            "activity",
            &json!({ "id": id, "title": contact["name"], "kind": "edited" }),
        );
        Ok(out)
    }

    pub fn delete(&self, id: &str, rev: u64) -> Outcome {
        sdk(self.client.call(
            "storage.delete",
            json!({ "collection": COLLECTION, "id": id, "rev": rev }),
        ))?;
        let _ = self.client.call("search.remove", json!({ "id": id }));
        Ok(json!({ "deleted": id }))
    }

    /// For other applications: who is this address, or who matches this
    /// name? Names, organisations, e-mail addresses and phone numbers only.
    pub fn lookup(&self, query: &str) -> Outcome {
        let q = fold(query.trim());
        if q.is_empty() {
            return Ok(json!({ "matches": [] }));
        }
        let mut matches: Vec<Value> = self
            .all()?
            .into_iter()
            .filter(|c| {
                let emails = c["emails"].as_array().cloned().unwrap_or_default();
                emails.iter().any(|e| fold(e["value"].as_str().unwrap_or_default()) == q)
                    || fold(c["name"].as_str().unwrap_or_default()).contains(&q)
                    || fold(c["org"].as_str().unwrap_or_default()).contains(&q)
            })
            .map(|c| json!({
                "id": c["id"], "name": c["name"], "org": c["org"],
                "emails": c["emails"].as_array().into_iter().flatten().filter_map(|e| e["value"].as_str()).collect::<Vec<_>>(),
                "phones": c["phones"].as_array().into_iter().flatten().filter_map(|e| e["value"].as_str()).collect::<Vec<_>>(),
            }))
            .collect();
        matches.sort_by_key(|c| sort_key(c["name"].as_str().unwrap_or_default()));
        matches.truncate(20);
        Ok(json!({ "matches": matches }))
    }

    pub fn export_vcf(&self, group: Option<&str>) -> Outcome {
        let people: Vec<Value> = self
            .all()?
            .into_iter()
            .filter(|c| {
                group.is_none_or(|g| {
                    c["groups"]
                        .as_array()
                        .is_some_and(|gs| gs.iter().any(|x| x == g))
                })
            })
            .collect();
        Ok(json!({ "vcf": vcard::export(&people), "count": people.len() }))
    }

    /// Import a .vcf; a card whose e-mail address is already known is skipped
    /// rather than duplicated.
    pub fn import_vcf(&self, text: &str) -> Outcome {
        let mut known: BTreeSet<String> = self
            .all()?
            .iter()
            .flat_map(|c| c["emails"].as_array().cloned().unwrap_or_default())
            .filter_map(|e| e["value"].as_str().map(fold))
            .collect();
        let (mut imported, mut duplicates, mut skipped) = (0, 0, 0);
        for card in vcard::import(text) {
            let emails: Vec<String> = card["emails"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e["value"].as_str().map(fold))
                .collect();
            if emails.iter().any(|e| known.contains(e)) {
                duplicates += 1;
                continue;
            }
            match self.create(&card) {
                Ok(_) => {
                    imported += 1;
                    known.extend(emails);
                }
                Err(_) => skipped += 1,
            }
        }
        Ok(json!({ "imported": imported, "duplicates": duplicates, "skipped": skipped }))
    }

    pub fn answer(&self, id: &Value, method: &str, params: &Value) -> Option<Value> {
        let (answer, show) = match method {
            "open" => {
                let contact = params["id"].as_str().unwrap_or_default();
                match self.get(contact) {
                    Ok(_) => (
                        Ok(json!({ "opened": contact })),
                        Some(json!({ "open": contact })),
                    ),
                    Err(e) => (Err(e), None),
                }
            }
            "focus" => (
                Ok(json!({ "focused": true })),
                Some(json!({ "focus": true })),
            ),
            "contact.lookup" => (
                self.lookup(params["query"].as_str().unwrap_or_default()),
                None,
            ),
            "contact.create" => {
                let mut fields = Map::new();
                for key in ["name", "given", "family", "org", "emails", "phones", "note"] {
                    if let Some(v) = params.get(key) {
                        fields.insert(key.into(), v.clone());
                    }
                }
                match self.create(&Value::Object(fields)) {
                    Ok(created) => {
                        let contact = created["id"].clone();
                        (Ok(created), Some(json!({ "open": contact })))
                    }
                    Err(e) => (Err(e), None),
                }
            }
            _ => (Err(fail("unknown_method", "not provided")), None),
        };
        let reply = answer.map_err(|e| {
            eon_office_sdk::AppError::new(
                e["code"].as_str().unwrap_or("error"),
                e["message"].as_str().unwrap_or_default(),
            )
        });
        let _ = self.client.answer(id, reply);
        show
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_sort_the_turkish_way() {
        let mut names = vec![
            "Zeynep", "Çağla", "Cem", "İrem", "Işıl", "Hakan", "Özge", "Oya", "Şule", "Selin",
            "Ümit", "Uğur", "Deniz",
        ];
        names.sort_by_key(|n| sort_key(n));
        assert_eq!(
            names,
            vec![
                "Cem", "Çağla", "Deniz", "Hakan", "Işıl", "İrem", "Oya", "Özge", "Selin", "Şule",
                "Uğur", "Ümit", "Zeynep"
            ]
        );
    }

    #[test]
    fn cleaning_rejects_what_is_not_a_contact() {
        assert!(clean(&json!({ "given": "Ada" })).is_ok_and(|c| c["name"] == "Ada"));
        assert!(clean(&json!({})).is_err());
        assert!(clean(&json!({ "name": "A", "emails": [{ "value": "not an address" }] })).is_err());
        assert!(clean(&json!({ "name": "A", "birthday": "23.04.1990" })).is_err());
        let c = clean(
            &json!({ "name": "A", "emails": [{ "value": " a@b.example " }, { "value": "" }], "groups": ["Work", "Work", " "] }),
        );
        let c = c.unwrap_or_default();
        assert_eq!(
            (c["emails"].as_array().map(Vec::len), c["groups"].clone()),
            (Some(1), json!(["Work"]))
        );
    }
}
