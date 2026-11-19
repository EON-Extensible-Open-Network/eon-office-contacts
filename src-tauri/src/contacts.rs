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
use eon_office_core::manifest::Manifest;
use eon_office_core::{links as eon_links, AppContext, Home};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

pub const COLLECTION: &str = "contacts";
pub type Outcome = Result<Value, Value>;

fn fail(code: &str, message: impl Into<String>) -> Value {
    json!({ "code": code, "message": message.into() })
}

/// A library error as the interface sees it: `{ code, message, data }`.
pub fn core<T>(result: eon_office_core::Result<T>) -> Result<T, Value> {
    result.map_err(|e| {
        let error = e.to_json();
        json!({ "code": error["data"]["code"], "message": error["message"], "data": error["data"] })
    })
}

/// What other applications may read about a contact: who it is and how to
/// reach them. Never the note, the address, the birthday or the groups.
pub fn summary(id: &str, contact: &Value) -> Value {
    let values = |key: &str| {
        contact[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| e["value"].as_str())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    json!({
        "id": id, "name": contact["name"], "org": contact["org"],
        "emails": values("emails"), "phones": values("phones"),
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
    ctx: Mutex<AppContext>,
}

impl Contacts {
    /// Open Contacts' store in `home`, registering Contacts with `folder` as
    /// where its program lives (`None`: leave the registration as it is).
    /// Contacts kept before summaries were published are published once.
    pub fn open(home: &Home, folder: Option<&Path>) -> Result<Contacts, Value> {
        let manifest: Value = serde_json::from_str(crate::MANIFEST)
            .map_err(|e| fail("internal", format!("manifest.json: {e}")))?;
        let manifest = core(Manifest::from_json(manifest))?;
        let contacts = Contacts {
            ctx: Mutex::new(core(home.app(&manifest, folder))?),
        };
        if contacts.ctx().settings_get("published").ok() != Some(json!(1)) {
            for c in contacts.all()? {
                let id = c["id"].as_str().unwrap_or_default().to_owned();
                let _ = contacts
                    .ctx()
                    .publish(&format!("contact:{id}"), &summary(&id, &c));
            }
            let _ = contacts.ctx().settings_set("published", &json!(1));
        }
        Ok(contacts)
    }

    fn ctx(&self) -> MutexGuard<'_, AppContext> {
        self.ctx.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The person's preferences: language, theme, text size, motion, contrast.
    pub fn prefs(&self) -> Outcome {
        core(self.ctx().prefs())
    }

    /// Changes whenever a preference changes; the window polls it.
    pub fn prefs_version(&self) -> i64 {
        self.ctx().prefs_version().unwrap_or(0)
    }

    fn activity(&self, id: &str, title: &Value, kind: &str) {
        let _ = self
            .ctx()
            .activity(id, title.as_str().unwrap_or_default(), kind);
    }

    fn find(&self, id: &str) -> Result<Option<Value>, Value> {
        let found = core(self.ctx().get(COLLECTION, id))?;
        Ok((!found.is_null()).then_some(found))
    }

    fn all(&self) -> Result<Vec<Value>, Value> {
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let page = core(self.ctx().list(COLLECTION, after.as_deref(), 1000))?;
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
        let found = self
            .find(id)?
            .ok_or_else(|| fail("not_found", "no such contact"))?;
        let mut c = found["doc"].clone();
        c["id"] = json!(id);
        c["rev"] = found["rev"].clone();
        self.activity(id, &c["name"], "opened");
        Ok(c)
    }

    fn write(&self, id: &str, contact: &Value, rev: Option<u64>) -> Outcome {
        let written = core(self.ctx().put(COLLECTION, id, contact, rev))?;
        let mut text: Vec<String> = Vec::new();
        for key in ["org", "title", "note"] {
            text.push(contact[key].as_str().unwrap_or_default().to_owned());
        }
        for key in ["emails", "phones"] {
            for item in contact[key].as_array().into_iter().flatten() {
                text.push(item["value"].as_str().unwrap_or_default().to_owned());
            }
        }
        let _ = self.ctx().index(
            id,
            contact["name"].as_str().unwrap_or_default(),
            &text.join(" "),
            false,
        );
        let _ = self
            .ctx()
            .publish(&format!("contact:{id}"), &summary(id, contact));
        Ok(json!({ "id": id, "rev": written["rev"] }))
    }

    pub fn create(&self, fields: &Value) -> Outcome {
        let mut contact = clean(fields)?;
        contact["created"] = json!(now_ms());
        contact["updated"] = json!(now_ms());
        let id = new_id();
        let out = self.write(&id, &contact, Some(0))?;
        self.activity(&id, &contact["name"], "created");
        Ok(out)
    }

    pub fn update(&self, id: &str, fields: &Value, rev: u64) -> Outcome {
        let existing = self
            .find(id)?
            .ok_or_else(|| fail("not_found", "no such contact"))?;
        let mut contact = clean(fields)?;
        contact["created"] = existing["doc"]["created"].clone();
        contact["updated"] = json!(now_ms());
        let out = self.write(id, &contact, Some(rev))?;
        self.activity(id, &contact["name"], "edited");
        Ok(out)
    }

    pub fn delete(&self, id: &str, rev: u64) -> Outcome {
        core(self.ctx().delete(COLLECTION, id, Some(rev)))?;
        let _ = self.ctx().unindex(id);
        let _ = self.ctx().unpublish(&format!("contact:{id}"));
        Ok(json!({ "deleted": id }))
    }

    /// Who is this address, or who matches this name? Names, organisations,
    /// e-mail addresses and phone numbers only. (Other applications read the
    /// same summaries from what Contacts publishes.)
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

    /// What the window should do for an `eon-office://` link it was started
    /// with or handed: open a contact by id, or come forward. Anything else is
    /// nothing.
    pub fn link(&self, text: &str) -> Option<Value> {
        let link = eon_links::parse(text)?;
        if link.app != crate::APP_ID {
            return None;
        }
        if link.item.is_empty() {
            return Some(json!({ "focus": true }));
        }
        self.find(&link.item)
            .ok()
            .flatten()
            .map(|_| json!({ "open": link.item }))
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
