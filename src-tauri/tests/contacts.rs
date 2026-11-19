// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! EON Contacts against a real core: the roadmap's acceptance criteria that a
//! machine can check. Needs `EON_OFFICE_CORE_BIN`.

#![allow(
    clippy::too_many_lines,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::format_push_string
)]

use eon_contacts::contacts::Contacts;
use eon_office_sdk::{Client, Incoming};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Core {
    child: Child,
    home: PathBuf,
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn start(name: &str) -> (Core, Client, Contacts) {
    let binary = std::env::var_os("EON_OFFICE_CORE_BIN").expect("set EON_OFFICE_CORE_BIN");
    let home = std::env::temp_dir().join(format!("eon-contacts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let child = Command::new(binary)
        .args(["serve", "--home"])
        .arg(&home)
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while eon_office_sdk::endpoint(&home).is_err() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(25));
    }
    let owner = Client::connect_owner(&home).unwrap();
    let manifest: Value = serde_json::from_str(eon_contacts::MANIFEST).unwrap();
    owner
        .call(
            "apps.install",
            json!({ "manifest": manifest, "grant": manifest["capabilities"] }),
        )
        .unwrap();
    let token = std::fs::read_to_string(home.join("apps").join(eon_contacts::APP_ID).join("token"))
        .unwrap();
    let client = Client::connect(
        &eon_office_sdk::endpoint(&home).unwrap(),
        Some(eon_contacts::APP_ID),
        token.trim(),
    )
    .unwrap();
    (Core { child, home }, owner, Contacts::new(client))
}

#[test]
#[ignore = "needs EON_OFFICE_CORE_BIN"]
fn contacts_sort_find_and_refuse_stale_saves() {
    let (_core, _owner, contacts) = start("basic");
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
#[ignore = "needs EON_OFFICE_CORE_BIN"]
fn vcards_from_outlook_and_google_import_without_duplicates() {
    let (_core, _owner, contacts) = start("vcf");
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
    let (_core2, _owner2, fresh) = start("vcf2");
    assert_eq!(
        fresh.import_vcf(out["vcf"].as_str().unwrap()).unwrap()["imported"],
        3
    );
}

#[test]
#[ignore = "needs EON_OFFICE_CORE_BIN"]
fn another_application_looks_up_a_sender_through_the_core() {
    let (_core, owner, contacts) = start("lookup");
    contacts.create(&json!({ "name": "Ayşe Yılmaz", "emails": [{ "value": "ayse@example.org" }], "phones": [{ "value": "+90 555" }], "note": "private" })).unwrap();
    let answering = std::thread::spawn(move || {
        while let Some(message) = contacts.client.next(Some(Duration::from_secs(10))) {
            if let Incoming::Invoke {
                id, method, params, ..
            } = message
            {
                return contacts.answer(&id, &method, &params);
            }
        }
        panic!("no invoke");
    });
    let found = owner
        .call_app(
            eon_contacts::APP_ID,
            "contact.lookup",
            json!({ "query": "AYSE@example.org" }),
        )
        .unwrap();
    assert!(
        answering.join().unwrap().is_none(),
        "a lookup shows nothing in the window"
    );
    assert_eq!(found["matches"][0]["name"], "Ayşe Yılmaz");
    assert_eq!(found["matches"][0]["phones"], json!(["+90 555"]));
    assert!(found["matches"][0].get("note").is_none());
}
