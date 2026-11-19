<!--
SPDX-License-Identifier: GPL-3.0-or-later
SPDX-FileCopyrightText: 2026 EON contributors
-->

# EON Contacts

**The people you work and live with, in one place your other applications can ask.**
Names, organisations, e-mail addresses, phone numbers, birthdays, notes and groups; vCard
import from Outlook, Google, Apple and Thunderbird, and export back; and a lookup other EON
Office applications use — EON Mail to name a sender, EON Calendar to find an attendee — that
hands them a summary, never the whole card. Part of [EON Office](https://github.com/EON-Extensible-Open-Network/eon-docs/blob/main/office/roadmap.md)'s Wave 1.

> **0.1.0** — protocol `v0`, early. **CardDAV sync is not in this version**; it needs the
> credential storage in the operating system's keychain designed for EON Mail, and moves to
> `0.2` with it (roadmap §6 records the change).

## What it does

- **A list sorted the Turkish way** — `Cem`, `Çağla`, `Deniz`; `Işıl`, `İrem` — and search
  that finds `Ayşe` when you type `AYSE`.
- **Cards** with several e-mail addresses and phone numbers, each with a kind (home, work,
  mobile, other) and its own *remove* button; birthday, web site, notes, groups.
- **vCard import** reads 2.1 (old Outlook, including quoted-printable), 3.0 (Google, Apple)
  and 4.0 (RFC 6350); a card whose e-mail address you already have is not duplicated.
  **Export** writes 3.0, which every one of those reads.
- **Two windows never overwrite each other:** a save on an out-of-date version is refused.
- **For other applications** (if you allow them `call:org.eon.office.contacts`):
  `contact.lookup` by address or part of a name returns names, organisations, addresses and
  numbers only; `contact.create`, `open`, `focus`.
- **Keyboard:** arrows, Home and End in the list, Enter to the card, Ctrl+N for a new
  contact. Follows EON Launcher's language, theme, text size, contrast and motion
  preferences, live.

**What it asks for:** `storage`, `search.index`, `search.query`, `events.publish`.

## Develop

```
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
EON_OFFICE_CORE_BIN=/path/to/eon-office-core cargo test -- --include-ignored
node tools/ui-check.mjs --contacts target/debug/eon-contacts.exe --core /path/to/eon-office-core.exe --shots shots
```

Licence `GPL-3.0-or-later` with the [EON Module ABI Exception](LICENSE-EXCEPTION.md).
Contributing: [CONTRIBUTING.md](CONTRIBUTING.md). Security: [SECURITY.md](SECURITY.md).
