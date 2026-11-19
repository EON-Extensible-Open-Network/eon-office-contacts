**EON Contacts** — people and organisations with groups, vCard import and export, and lookup for other applications, for EON Office.
Early (protocol `v0`): for trying out.

**Install:** the simplest way is EON Launcher → *Get applications* (with previews on), which
downloads the right file, checks it against the index, and installs it. By hand: put the
file for your platform in a folder, rename it to `eon-contacts` (`eon-contacts.exe` on Windows),
put the manifest beside it as `manifest.json`, and drop that manifest onto EON Launcher.
Then allow its permissions.

**Not signed** — EON has no signing keys yet (plan, madde 30 and 34). Check the files against
`SHA256SUMS.txt`. On Windows, Smart App Control (if on) refuses unsigned downloads.

No telemetry, no account. Your contacts live in the EON Office core on your device.
