// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! EON Contacts on the EON Office library, in a home folder of its own: the
//! roadmap's acceptance criteria that a machine can check. Nothing else has
//! to be installed or running.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::format_push_string
)]

use eon_contacts::contacts::Contacts;
use eon_office_core::manifest::Manifest;
use eon_office_core::Home;
use serde_json::json;
use std::path::PathBuf;

struct TempHome(PathBuf);

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn start(name: &str) -> (TempHome, Home, Contacts) {
    let dir = std::env::temp_dir().join(format!("eon-contacts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let home = Home::open(&dir).unwrap();
    let contacts = Contacts::open(&home, None).unwrap();
    (TempHome(dir), home, contacts)
}

#[test]
fn contacts_sort_find_and_refuse_stale_saves() {
    let (_dir, _home, contacts) = start("basic");
    for (name, email, org) in [
        ("Zeynep Kaya", "zeynep@example.org", "Acme"),
        ("Çağla Demir", "cagla@example.org", ""),
        ("Cem Ak", "cem@example.org", "Acme"),
        ("İrem Su", "irem@example.org", ""),
    ] {
        contacts.create(&json!({ "name": name, "emails": [{ "type": "work", "value": email }], "org": org })).unwrap();
    }
    assert!(contacts
        .create(&json!({ "name": "Bad", "emails": [{ "value": "no-at-sign" }] }))
        .is_err());
    let list = contacts.list(None).unwrap();
    let names: Vec<&str> = list["contacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec!["Cem Ak", "Çağla Demir", "İrem Su", "Zeynep Kaya"]
    );

    // Lookup by address (exact, case-folded), by part of a name (Turkish-folded), by organisation.
    let by_mail = contacts.lookup("CAGLA@example.org").unwrap();
    assert_eq!(by_mail["matches"][0]["name"], "Çağla Demir");
    let by_name = contacts.lookup("irem").unwrap();
    assert_eq!(by_name["matches"][0]["name"], "İrem Su");
    assert_eq!(
        contacts.lookup("acme").unwrap()["matches"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        by_mail["matches"][0].get("note").is_none(),
        "lookup returns a summary, not the card"
    );

    let id = list["contacts"][0]["id"].as_str().unwrap().to_owned();
    let c = contacts.get(&id).unwrap();
    let rev = c["rev"].as_u64().unwrap();
    contacts
        .update(&id, &json!({ "name": "Cem Ak", "groups": ["Work"] }), rev)
        .unwrap();
    assert_eq!(
        contacts
            .update(&id, &json!({ "name": "Cem A." }), rev)
            .unwrap_err()["code"],
        "conflict"
    );
    assert_eq!(
        contacts.list(Some("Work")).unwrap()["contacts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn vcards_from_outlook_and_google_import_without_duplicates() {
    let (_dir, _home, contacts) = start("vcf");
    contacts
        .create(&json!({ "name": "Grace Hopper", "emails": [{ "value": "grace@navy.example" }] }))
        .unwrap();
    let outlook = "BEGIN:VCARD\r\nVERSION:2.1\r\nN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:Y=C4=B1lmaz;Ay=C5=9Fe\r\nEMAIL;PREF;INTERNET:ayse@example.org\r\nTEL;WORK;VOICE:+90 212 000 00 00\r\nEND:VCARD\r\n";
    let google = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Grace Hopper\r\nEMAIL;TYPE=INTERNET:GRACE@navy.example\r\nEND:VCARD\r\nBEGIN:VCARD\r\nVERSION:3.0\r\nFN:Ada Lovelace\r\nEMAIL;TYPE=INTERNET:ada@example.org\r\nCATEGORIES:Friends\r\nEND:VCARD\r\n";
    let report = contacts.import_vcf(&format!("{outlook}{google}")).unwrap();
    assert_eq!(
        (report["imported"].clone(), report["duplicates"].clone()),
        (json!(2), json!(1))
    );
    let all = contacts.list(None).unwrap();
    let names: Vec<&str> = all["contacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Ada Lovelace", "Ayşe Yılmaz", "Grace Hopper"]);
    assert_eq!(all["groups"]["Friends"], 1);

    // Export, and the file reads back into an empty book with the same people.
    let out = contacts.export_vcf(None).unwrap();
    assert_eq!(out["count"], 3);
    let (_dir2, _home2, fresh) = start("vcf2");
    assert_eq!(
        fresh.import_vcf(out["vcf"].as_str().unwrap()).unwrap()["imported"],
        3
    );
}

#[test]
fn another_app_reads_published_summaries_not_notes() {
    let (_dir, home, contacts) = start("public");
    let made = contacts.create(&json!({ "name": "Ayşe Yılmaz", "org": "Acme", "emails": [{ "value": "ayse@example.org" }], "phones": [{ "value": "+90 555" }], "note": "private" })).unwrap();
    let id = made["id"].as_str().unwrap();
    // Mail or Calendar, declaring call:org.eon.office.contacts, reads the summaries.
    let mail = home
        .app(
            &Manifest::from_json(json!({
                "manifest": 0, "id": "org.test.mail", "name": "Mail", "version": "0.1.0",
                "api": { "min": 0, "max": 0 }, "capabilities": ["call:org.eon.office.contacts"],
            }))
            .unwrap(),
            None,
        )
        .unwrap();
    let seen = mail.public_of(eon_contacts::APP_ID).unwrap();
    assert_eq!(seen.len(), 1);
    let (key, summary) = &seen[0];
    assert_eq!(key, &format!("contact:{id}"));
    assert_eq!(
        summary,
        &json!({ "id": id, "name": "Ayşe Yılmaz", "org": "Acme", "emails": ["ayse@example.org"], "phones": ["+90 555"] })
    );
    // An edit is published again; a delete takes the summary away.
    let rev = contacts.get(id).unwrap()["rev"].as_u64().unwrap();
    contacts
        .update(
            id,
            &json!({ "name": "Ayşe Y.", "emails": [{ "value": "ayse@example.org" }] }),
            rev,
        )
        .unwrap();
    assert_eq!(
        mail.public_of(eon_contacts::APP_ID).unwrap()[0].1["name"],
        "Ayşe Y."
    );
    let rev = contacts.get(id).unwrap()["rev"].as_u64().unwrap();
    contacts.delete(id, rev).unwrap();
    assert!(mail.public_of(eon_contacts::APP_ID).unwrap().is_empty());
}

#[test]
fn a_link_argument_opens_the_contact() {
    let (_dir, _home, contacts) = start("link");
    let made = contacts.create(&json!({ "name": "Linked" })).unwrap();
    let id = made["id"].as_str().unwrap();
    assert_eq!(
        contacts.link(&format!("eon-office://org.eon.office.contacts/{id}")),
        Some(json!({ "open": id }))
    );
    assert_eq!(
        contacts.link("eon-office://org.eon.office.contacts"),
        Some(json!({ "focus": true }))
    );
    assert_eq!(contacts.link("eon-office://org.eon.office.notes/x"), None);
    assert_eq!(
        contacts.link("eon-office://org.eon.office.contacts/nope"),
        None
    );
}
