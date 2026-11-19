// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Drives the real EON Contacts window through the WebView2 DevTools protocol.
// The core is started in a private home, Contacts is installed and launched by
// the core -- as a person's launcher would -- and the window is checked.
// No dependencies (Node 22+), Windows, debug build of Contacts.
//
//   node tools/ui-check.mjs --contacts <eon-contacts.exe> --core <eon-office-core.exe> [--shots <dir>]

import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, existsSync, rmSync } from "node:fs";
import { createConnection } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const arg = (name) => { const at = process.argv.indexOf(name); return at > 0 ? process.argv[at + 1] : undefined; };
const appExe = arg("--contacts") && resolve(arg("--contacts"));
const coreExe = arg("--core") && resolve(arg("--core"));
const shots = arg("--shots");
if (!appExe || !coreExe) { console.error("usage: node tools/ui-check.mjs --contacts <exe> --core <exe> [--shots <dir>]"); process.exit(2); }
if (shots) mkdirSync(shots, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const home = mkdtempSync(join(tmpdir(), "eon-contacts-ui-"));
const port = 9300 + Math.floor(Math.random() * 500);
let failures = 0;
const check = (name, ok, detail = "") => {
  console.log(`  ${ok ? "ok  " : "FAIL"}  ${name}${ok || !detail ? "" : `: ${detail}`}`);
  if (!ok) failures++;
};

function owner() {
  const endpoint = JSON.parse(readFileSync(join(home, "runtime", "endpoint.json"), "utf8"));
  const [host, p] = endpoint.address.split(":");
  const socket = createConnection({ host, port: Number(p) });
  let buffer = ""; let next = 1; const waiting = new Map();
  socket.setEncoding("utf8");
  socket.on("data", (chunk) => {
    buffer += chunk; let at;
    while ((at = buffer.indexOf("\n")) >= 0) {
      const m = JSON.parse(buffer.slice(0, at)); buffer = buffer.slice(at + 1);
      if (!m.method && waiting.has(m.id)) { waiting.get(m.id)(m); waiting.delete(m.id); }
    }
  });
  const call = (method, params = {}) => new Promise((ok, no) => {
    const id = next++;
    waiting.set(id, (m) => (m.error ? no(m.error) : ok(m.result)));
    socket.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
  });
  const token = readFileSync(join(home, "runtime", "owner.token"), "utf8").trim();
  const ready = new Promise((r) => socket.on("connect", r)).then(() => call("session.hello", { token }));
  return { call, ready, close: () => socket.destroy() };
}

let cdp;
async function devtools() {
  for (let i = 0; i < 300; i++) {
    try {
      const pages = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      const page = pages.find((p) => p.type === "page" && p.webSocketDebuggerUrl);
      if (page) {
        const ws = new WebSocket(page.webSocketDebuggerUrl);
        await new Promise((ok, no) => { ws.onopen = ok; ws.onerror = no; });
        let next = 1; const waiting = new Map();
        ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && waiting.has(m.id)) { waiting.get(m.id)(m); waiting.delete(m.id); } };
        return { send: (method, params = {}) => new Promise((ok) => { const id = next++; waiting.set(id, ok); ws.send(JSON.stringify({ id, method, params })); }), close: () => ws.close() };
      }
    } catch { /* not yet */ }
    await sleep(200);
  }
  throw new Error("the Contacts window's DevTools endpoint never appeared");
}
async function js(expression) {
  const r = await cdp.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails));
  return r.result?.result?.value;
}
async function until(expression, ms = 8000) {
  const end = Date.now() + ms;
  while (Date.now() < end) { if (await js(expression).catch(() => false)) return true; await sleep(100); }
  return false;
}
async function key(keyName, code = keyName, modifiers = 0, text) {
  const vk = { ArrowDown: 40, ArrowUp: 38, Home: 36, End: 35, ArrowRight: 39, ArrowLeft: 37, Enter: 13 }[keyName] ?? keyName.toUpperCase().charCodeAt(0);
  // Shortcuts with modifiers are raw key presses, as CDP expects; plain keys may carry text.
  await cdp.send("Input.dispatchKeyEvent", { type: modifiers ? "rawKeyDown" : "keyDown", key: keyName, code, modifiers, windowsVirtualKeyCode: vk, text });
  await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: keyName, code, modifiers, windowsVirtualKeyCode: vk });
}
// Ctrl+ shortcuts are sent as page-level keyboard events. Synthetic OS-level
// presses through CDP reach a background-started webview only intermittently,
// which tests the delivery, not Notes; arrows still go through CDP below.
async function shortcut(letter) {
  await js(`document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key: '${letter}', ctrlKey: true, bubbles: true, cancelable: true }))`);
}
async function type(text) { await cdp.send("Input.insertText", { text }); }
async function shot(name) {
  if (!shots) return;
  const r = await cdp.send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(shots, `contacts-${name}.png`), Buffer.from(r.result.data, "base64"));
}

const core = spawn(coreExe, ["serve", "--home", home], {
  env: { ...process.env, EON_WEBVIEW_ARGS: `--remote-debugging-port=${port}` },
  stdio: "ignore",
});
let o;
try {
  console.log("EON Contacts interface check");
  for (let i = 0; i < 100 && !existsSync(join(home, "runtime", "endpoint.json")); i++) await sleep(100);
  o = owner();
  await o.ready;
  await o.call("prefs.set", { key: "locale", value: "en" });
  const manifest = JSON.parse(readFileSync(new URL("../manifest.json", import.meta.url), "utf8"));
  manifest.entry = { command: appExe };
  await o.call("apps.install", { manifest, grant: manifest.capabilities });
  await o.call("apps.launch", { app: manifest.id });
  cdp = await devtools();
  await cdp.send("Emulation.setFocusEmulationEnabled", { enabled: true });
  const setv = (id, v) => `document.getElementById('${id}').value = ${JSON.stringify(v)};`;

  check("the window connects with an empty book", await until(`document.body.dataset.ready === '1' && !document.getElementById('empty').hidden`, 20000));
  await js(`document.getElementById('new-contact').click()`);
  check("New contact opens a blank card, name focused", await until(`!document.getElementById('card').hidden && document.activeElement.id === 'c-name'`));
  check("the card starts with one e-mail and one phone row", await js(`document.querySelectorAll('#emails li').length === 1 && document.querySelectorAll('#phones li').length === 1`));
  await js(`(() => { ${setv("c-name", "Ayşe Yılmaz")} ${setv("c-org", "Acme")} document.querySelector('#emails input').value = 'ayse@example.org'; document.querySelector('#phones input').value = '+90 555 000 00 00'; ${setv("c-groups", "Work, Friends")} document.getElementById('card').requestSubmit(); })()`);
  check("saving adds the person to the list", await until(`document.getElementById('contact-list').textContent.includes('Ayşe Yılmaz')`));
  check("groups appear in the filter", await until(`[...document.getElementById('group-filter').options].some(o => o.value === 'Work')`));
  await shot("01-card");

  await js(`document.getElementById('add-email').click()`);
  check("Add an e-mail address adds a row and focuses it", await until(`document.querySelectorAll('#emails li').length === 2 && document.activeElement.type === 'email'`));
  await js(`document.querySelectorAll('#emails li')[1].querySelector('button').click()`);
  check("a row can be removed with its own button", await until(`document.querySelectorAll('#emails li').length === 1`));
  await js(`(() => { document.querySelector('#emails input').value = 'not an address'; document.getElementById('card').requestSubmit(); })()`);
  check("an address without @ is refused with a reason", await until(`document.getElementById('save-status').textContent.includes('not an e-mail address')`));
  await js(`(() => { document.querySelector('#emails input').value = 'ayse@example.org'; document.getElementById('card').requestSubmit(); })()`);
  await until(`document.getElementById('save-status').textContent === 'Saved'`);

  await js(`document.getElementById('new-contact').click()`);
  await js(`(() => { ${setv("c-name", "Cem Ak")} document.querySelector('#emails input').value = 'cem@example.org'; document.getElementById('card').requestSubmit(); })()`);
  check("the list sorts the Turkish way", await until(`[...document.querySelectorAll('#contact-list li')].map(li => li.firstChild.textContent).join('|') === 'Ayşe Yılmaz|Cem Ak'`));

  await js(`(() => { const q = document.getElementById('search'); q.value = 'AYSE'; q.dispatchEvent(new Event('input')); })()`);
  check("search finds Turkish names typed without Turkish letters", await until(`document.querySelectorAll('#contact-list li').length === 1 && document.getElementById('contact-list').textContent.includes('Ayşe')`));
  await js(`(() => { const q = document.getElementById('search'); q.value = ''; q.dispatchEvent(new Event('input')); })()`);

  await js(`document.getElementById('contact-list').focus()`);
  await key("Home");
  check("Home in the list opens the first contact", await until(`document.getElementById('c-name').value === 'Ayşe Yılmaz'`));
  await key("ArrowDown");
  check("ArrowDown opens the next", await until(`document.getElementById('c-name').value === 'Cem Ak'`));

  const found = await o.call("apps.call", { app: manifest.id, method: "contact.lookup", params: { query: "cem@example.org" } });
  check("another app's lookup finds the contact through the core", found.matches?.[0]?.name === "Cem Ak");

  await js(`document.getElementById('delete').click()`);
  check("Delete asks first", await until(`!document.getElementById('delete-confirm').hidden && document.activeElement.id === 'delete-no'`));
  await js(`document.getElementById('delete-yes').click()`);
  check("confirming deletes", await until(`!document.getElementById('contact-list').textContent.includes('Cem Ak')`));

  await o.call("prefs.set", { key: "locale", value: "tr" });
  check("a language change reaches Contacts, including built text", await until(`document.getElementById('new-contact').textContent === 'Yeni kişi' && document.getElementById('count').textContent.endsWith('kişi') && document.getElementById('group-filter').options[0].textContent === 'Herkes'`));
  await shot("02-turkish");

  const unnamed = await js(`[...document.querySelectorAll('button, input, select, textarea')].filter(e => {
    if (e.closest('[hidden]')) return false;
    const label = e.getAttribute('aria-label') || e.textContent.trim() || (e.id && document.querySelector('label[for="' + e.id + '"]')?.textContent.trim());
    return !label;
  }).map(e => e.outerHTML.slice(0, 80))`);
  check("every visible control has an accessible name", unnamed.length === 0, JSON.stringify(unnamed));
} catch (error) {
  failures++;
  console.error(`  FAIL  ${error.stack || error}`);
} finally {
  try { await o?.call("core.shutdown"); } catch { /* gone */ }
  o?.close();
  cdp?.close();
  await sleep(1500);
  try { core.kill(); } catch { /* gone */ }
  try { rmSync(home, { recursive: true, force: true }); } catch { /* in use */ }
}
console.log(failures ? `\n${failures} check(s) failed.` : "\nall checks passed");
process.exit(failures ? 1 : 0);
