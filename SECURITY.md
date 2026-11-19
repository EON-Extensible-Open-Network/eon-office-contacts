<!--
SPDX-License-Identifier: GPL-3.0-or-later
SPDX-FileCopyrightText: 2026 EON contributors
-->

# Security

Report privately through GitHub's **Security → Report a vulnerability** on this repository;
the organisation-wide policy is
[`.github/SECURITY.md`](https://github.com/EON-Extensible-Open-Network/.github/blob/main/SECURITY.md).

- **Imported vCards are untrusted.** It is parsed into plain fields; anything unknown is
  ignored, and text reaches the page only as text. The interface has no network (CI fails the build on `fetch`, `XMLHttpRequest`,
  `WebSocket`, `EventSource`, and on `innerHTML`).
- **Contacts opens its own store only** (`apps/org.eon.office.contacts/data.sqlite`) and
  declares four capabilities; the library refuses anything else. Links can only open an
  existing contact.
- **The DevTools port is debug-only** (`EON_WEBVIEW_ARGS`, ignored by release builds).
- Known, tracked: the Linux build inherits Tauri's GTK 3 stack and its `glib` advisory
  (GHSA-wrw7-89jp-8q8g), as described in eon-office-launcher's `SECURITY.md`.

- **Other applications get a summary.** What Contacts publishes is names, organisations,
  e-mail addresses and phone numbers; never notes, birthdays, postal addresses or groups. An
  application must declare `call:org.eon.office.contacts` to read it, and the person can
  revoke that.

The library's threat model, including what capabilities are and are not:
[`eon-office-core/SECURITY.md`](https://github.com/EON-Extensible-Open-Network/eon-office-core/blob/main/SECURITY.md).
