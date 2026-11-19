// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// EON Contacts' interface. No network of its own; text from contacts (which
// may be imported from anywhere) enters the page only through textContent.

"use strict";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = (id) => document.getElementById(id);

let strings = {};
let people = [];
let current = null;

function t(key, vars = {}) {
  let text = strings[key] ?? key;
  for (const [n, v] of Object.entries(vars)) text = text.replaceAll(`{${n}}`, String(v));
  return text;
}
const reason = (e) => (e && (e.message || e.code)) || String(e);

async function loadStrings(locale) {
  const language = locale && locale !== "system"
    ? (locale.startsWith("tr") ? "tr" : "en")
    : ((navigator.language || "en").toLowerCase().startsWith("tr") ? "tr" : "en");
  strings = await invoke("strings", { language });
  document.documentElement.lang = language;
  for (const n of document.querySelectorAll("[data-i18n]")) n.textContent = t(n.dataset.i18n);
  for (const n of document.querySelectorAll("[data-i18n-label]")) n.setAttribute("aria-label", t(n.dataset.i18nLabel));
  for (const n of document.querySelectorAll("[data-i18n-placeholder]")) n.setAttribute("placeholder", t(n.dataset.i18nPlaceholder));
}

async function applyPrefs() {
  const p = await invoke("prefs");
  const root = document.documentElement;
  if (p.theme === "light" || p.theme === "dark") root.dataset.theme = p.theme; else delete root.dataset.theme;
  if (p.high_contrast) root.dataset.contrast = "high"; else delete root.dataset.contrast;
  if (p.reduce_motion) root.dataset.motion = "reduce"; else delete root.dataset.motion;
  root.style.setProperty("--scale", String(p.font_scale || 1));
  await loadStrings(p.locale);
}

function announce(text) {
  $("announcer").textContent = "";
  window.setTimeout(() => { $("announcer").textContent = text; }, 30);
}

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else if (k === "text") node.textContent = v;
    else if (k.startsWith("on")) node.addEventListener(k.slice(2), v);
    else node.setAttribute(k, v);
  }
  for (const c of children) if (c !== null && c !== undefined && c !== false) node.append(c);
  return node;
}

// ---- the list ------------------------------------------------------------

function renderList(items = people) {
  $("count").textContent = t("count", { count: items.length });
  $("empty").hidden = people.length > 0 || current !== null;
  $("contact-list").replaceChildren(...items.map((c) => el("li", {
    role: "option", id: `opt-${c.id}`, "aria-selected": String(current?.id === c.id), onclick: () => openContact(c.id),
  }, el("div", { text: c.name }), el("div", { class: "meta", text: [c.org, c.email || c.phone].filter(Boolean).join(" · ") }))));
}

async function refresh() {
  const group = $("group-filter").value || null;
  const r = await invoke("list", { group });
  people = r.contacts;
  const chosen = $("group-filter").value;
  $("group-filter").replaceChildren(el("option", { value: "", text: t("group.all") }),
    ...Object.entries(r.groups).map(([g, n]) => el("option", { value: g, text: `${g} (${n})` })));
  $("group-filter").value = Object.keys(r.groups).includes(chosen) ? chosen : "";
  renderList();
}
$("group-filter").addEventListener("change", refresh);

$("contact-list").addEventListener("keydown", (e) => {
  if (!people.length) return;
  const at = Math.max(0, people.findIndex((c) => c.id === current?.id));
  const moves = { ArrowDown: at + 1, ArrowUp: at - 1, Home: 0, End: people.length - 1 };
  if (e.key in moves) {
    e.preventDefault();
    openContact(people[Math.min(people.length - 1, Math.max(0, moves[e.key]))].id, false);
  } else if (e.key === "Enter") {
    e.preventDefault();
    $("c-name").focus();
  }
});

let searchTimer = 0;
$("search").addEventListener("input", () => {
  window.clearTimeout(searchTimer);
  searchTimer = window.setTimeout(async () => {
    const q = $("search").value.trim();
    if (!q) { renderList(); return; }
    try {
      const { matches } = await invoke("search", { query: q });
      renderList(matches.map((m) => ({ id: m.id, name: m.name, org: m.org, email: m.emails[0], phone: m.phones[0] })));
    } catch (e) {
      announce(t("error", { reason: reason(e) }));
    }
  }, 200);
});

// ---- the card ----------------------------------------------------------------

const KINDS = ["home", "work", "cell", "other"];

function row(listId, item, valueLabel) {
  const type = el("select", { "aria-label": t("type") }, ...KINDS.map((k) => el("option", { value: k, text: t(`type.${k}`) })));
  type.value = KINDS.includes(item.type) ? item.type : "other";
  const value = el("input", { type: listId === "emails" ? "email" : "tel", "aria-label": valueLabel, autocomplete: "off" });
  value.value = item.value || "";
  const li = el("li", {}, type, value);
  li.append(el("button", { type: "button", class: "secondary", text: "×", "aria-label": t("remove", { what: valueLabel }), onclick: () => { li.remove(); } }));
  $(listId).append(li);
  return value;
}

function rows(listId) {
  return [...$(listId).querySelectorAll("li")].map((li) => ({ type: li.querySelector("select").value, value: li.querySelector("input").value }));
}

$("add-email").addEventListener("click", () => row("emails", {}, t("value.email")).focus());
$("add-phone").addEventListener("click", () => row("phones", {}, t("value.phone")).focus());

function fill(c) {
  for (const [id, key] of [["c-name", "name"], ["c-given", "given"], ["c-family", "family"], ["c-org", "org"], ["c-title", "title"], ["c-birthday", "birthday"], ["c-url", "url"], ["c-note", "note"]]) $(id).value = c[key] || "";
  $("c-groups").value = (c.groups || []).join(", ");
  $("emails").replaceChildren();
  $("phones").replaceChildren();
  for (const e of c.emails || []) row("emails", e, t("value.email"));
  for (const p of c.phones || []) row("phones", p, t("value.phone"));
}

async function openContact(id, focusName = true) {
  try {
    current = await invoke("get", { id });
  } catch (e) {
    announce(t("error", { reason: reason(e) }));
    return;
  }
  $("card").hidden = false;
  $("empty").hidden = true;
  $("delete-confirm").hidden = true;
  $("save-status").textContent = "";
  fill(current);
  renderList();
  if (focusName) $("c-name").focus();
}

function collect() {
  return {
    name: $("c-name").value, given: $("c-given").value, family: $("c-family").value, org: $("c-org").value,
    title: $("c-title").value, birthday: $("c-birthday").value, url: $("c-url").value, note: $("c-note").value,
    emails: rows("emails"), phones: rows("phones"),
    groups: $("c-groups").value.split(",").map((g) => g.trim()).filter(Boolean),
    addresses: current?.addresses || [],
  };
}

$("card").addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    if (current?.id) {
      const r = await invoke("update", { id: current.id, fields: collect(), rev: current.rev });
      current.rev = r.rev;
    } else {
      const r = await invoke("create", { fields: collect() });
      current = await invoke("get", { id: r.id });
    }
    $("save-status").textContent = t("saved");
    announce(t("saved"));
    await refresh();
  } catch (e) {
    const text = e && e.code === "conflict" ? t("conflict") : t("error", { reason: reason(e) });
    $("save-status").textContent = text;
    announce(text);
  }
});

$("new-contact").addEventListener("click", () => {
  current = null;
  fill({ emails: [{ type: "home" }], phones: [{ type: "cell" }] });
  $("card").hidden = false;
  $("empty").hidden = true;
  $("delete-confirm").hidden = true;
  $("save-status").textContent = "";
  renderList();
  $("c-name").focus();
});

$("delete").addEventListener("click", () => {
  if (!current?.id) { $("card").hidden = true; return; }
  $("delete-question").textContent = t("delete.confirm", { name: current.name });
  $("delete-confirm").hidden = false;
  $("delete-no").focus();
});
$("delete-no").addEventListener("click", () => { $("delete-confirm").hidden = true; $("delete").focus(); });
$("delete-yes").addEventListener("click", async () => {
  try {
    await invoke("delete", { id: current.id, rev: current.rev });
    current = null;
    $("card").hidden = true;
    await refresh();
    $("contact-list").focus();
  } catch (e) {
    announce(t("error", { reason: reason(e) }));
  }
});

// ---- vCard ------------------------------------------------------------------------

$("export-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    const r = await invoke("exportVcf", { path: $("export-path").value, group: $("group-filter").value || null });
    $("count").textContent = t("exported", { count: r.count, path: r.path });
    announce(t("exported", { count: r.count, path: r.path }));
  } catch (e) {
    announce(t("error", { reason: reason(e) }));
  }
});
$("import-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    const r = await invoke("importVcf", { path: $("import-path").value });
    await refresh();
    $("count").textContent = t("import.done", { count: r.imported, duplicates: r.duplicates });
    announce(t("import.done", { count: r.imported, duplicates: r.duplicates }));
  } catch (e) {
    announce(t("error", { reason: reason(e) }));
  }
});

// ---- links and preference changes ---------------------------------------------------------------------

async function show(payload) {
  if (!payload) return;
  await refresh();
  if (payload.open) await openContact(payload.open);
}
listen("show", ({ payload }) => show(payload));
listen("prefs-changed", async () => {
  await applyPrefs();
  await refresh();
  if (current) fill(collect());
});
document.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") { e.preventDefault(); $("new-contact").click(); }
});

async function start() {
  await loadStrings("system");
  try {
    await invoke("status");
    await applyPrefs();
    $("banner").hidden = true;
    $("layout").hidden = false;
    await refresh();
    if (document.activeElement === document.body || document.activeElement === null) $("contact-list").focus();
    await show(await invoke("initial"));
    document.body.dataset.ready = "1";
  } catch (e) {
    $("layout").hidden = true;
    $("banner").hidden = false;
    $("banner-text").textContent = t("open.failed", { reason: reason(e) });
  }
}
$("retry").addEventListener("click", start);
start();
