<!--
SPDX-License-Identifier: GPL-3.0-or-later
SPDX-FileCopyrightText: 2026 EON contributors
-->

# Changelog

## 0.2.0 — first release, standing alone

`0.1.0` was built and tested against the EON Office core service but never released; the
service was replaced before it was tagged (plan madde 43;
`eon-docs/office/design/2026-11-19-library-core.md`). Everything below `0.1.0` is in this
release, with these changes:

- **No core service, no launcher needed.** Contacts links the `eon-office-core` library
  and opens its own store in the EON Office home folder.
- **Published summaries replace `contact.lookup`:** every save publishes the person's name,
  organisation, e-mail addresses and phone numbers for applications that declare
  `call:org.eon.office.contacts`; a delete withdraws it. Contacts kept before this are
  published once at the first start.
- **One window:** a second start, or an `eon-office://org.eon.office.contacts/<id>` link,
  goes to the running window. Replaces `open`, `focus` and `contact.create`.
- **Export and import vCard work from the window.** The buttons invoked
  `exportVcf`/`importVcf`, names the host never registered (Tauri matches the Rust
  function name, `export_vcf`), so both failed when pressed. The self-test now fails the
  build when the interface invokes a command the host does not register, and the window
  check exports and re-imports through the forms.

## 0.1.0 — first version

- Contacts with several e-mail addresses and phone numbers, organisation, title, birthday,
  web site, notes and groups; a list sorted in Turkish alphabetical order; Turkish-folded
  search.
- vCard import (2.1 with quoted-printable, 3.0, 4.0), skipping cards whose e-mail address is
  already known; vCard 3.0 export.
- Saves refused on an out-of-date version.
- Provides `contact.lookup` (a summary, never the whole card), `contact.create`, `open`,
  `focus`; publishes `activity`.
- Not yet: CardDAV sync, moved to 0.2 with keychain credential storage.
