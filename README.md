<!--
SPDX-License-Identifier: GPL-3.0-or-later
SPDX-FileCopyrightText: 2026 EON contributors
-->

# EON Contacts

**The people you work and live with, in one place your other applications can ask.**
Names, organisations, e-mail addresses, phone numbers, birthdays, notes and groups; vCard
import from Outlook, Google, Apple and Thunderbird, and export back; and a summary other EON
Office applications may read — EON Mail to name a sender, EON Calendar to find an attendee —
never the whole card. Part of [EON Office](https://github.com/EON-Extensible-Open-Network/eon-docs/blob/main/office/roadmap.md)'s Wave 1.

> **0.2.0** — formats `v0`, early. **Stands alone:** download it, run it. Nothing else
> has to be installed or running. **CardDAV sync is not in this version**; it needs the
> credential storage in the operating system's keychain designed for EON Mail, and comes
> with it (roadmap §6 records the change).

## What it does

- **A list sorted the Turkish way** — `Cem`, `Çağla`, `Deniz`; `Işıl`, `İrem` — and search
  that finds `Ayşe` when you type `AYSE`.
- **Cards** with several e-mail addresses and phone numbers, each with a kind (home, work,
  mobile, other) and its own *remove* button; birthday, web site, notes, groups.
- **vCard import** reads 2.1 (old Outlook, including quoted-printable), 3.0 (Google, Apple)
  and 4.0 (RFC 6350); a card whose e-mail address you already have is not duplicated.
  **Export** writes 3.0, which every one of those reads.
- **Two windows never overwrite each other:** a save on an out-of-date version is refused.
- **Keyboard:** arrows, Home and End in the list, Enter to the card, Ctrl+N for a new
  contact. Follows the language, theme, text size, contrast and motion preferences shared by
  EON Office applications, live.
- **One window.** Starting Contacts again, or following a link to a contact, brings the
  open window forward.

**With other EON Office applications** (all optional): Contacts publishes a summary of each
person — name, organisation, e-mail addresses, phone numbers — that applications you allow
(`call:org.eon.office.contacts`) may read; notes, birthdays, addresses and groups stay in
Contacts. `eon-office://org.eon.office.contacts/<id>` opens a contact.

**What it asks for:** keep its data on this device (`storage`), make contacts searchable
(`search.index`, private to Contacts), search (`search.query`), add names to your recent list
and publish the summaries (`events.publish`).

## Install

Download `eon-contacts` from a release and run it — see the release notes for checksums.
Or, with EON Launcher: *Get applications* → *Include previews* → EON Contacts.

## Develop

```
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
node tools/ui-check.mjs --contacts target/debug/eon-contacts.exe --shots shots
```

Licence `GPL-3.0-or-later` with the [EON Module ABI Exception](LICENSE-EXCEPTION.md).
Contributing: [CONTRIBUTING.md](CONTRIBUTING.md). Security: [SECURITY.md](SECURITY.md).
