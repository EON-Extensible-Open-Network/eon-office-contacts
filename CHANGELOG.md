<!--
SPDX-License-Identifier: GPL-3.0-or-later
SPDX-FileCopyrightText: 2026 EON contributors
-->

# Changelog

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
