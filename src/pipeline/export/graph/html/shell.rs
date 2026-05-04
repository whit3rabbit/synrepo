//! HTML shell for the graph export.

pub(super) const HTML_PREFIX: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>synrepo graph</title>
<style>
:root {
  color-scheme: light;
  --bg: #f7f7f3;
  --panel: #ffffff;
  --ink: #1e2428;
  --muted: #5e676f;
  --line: #d5d9dc;
  --soft: #eef2f7;
  --accent: #2563eb;
  --drift: #b91c1c;
  --file: #2563eb;
  --symbol: #0f766e;
  --concept: #a16207;
  --shadow: 0 8px 28px rgba(20, 31, 41, 0.08);
}
* { box-sizing: border-box; }
body {
  margin: 0;
  min-height: 100vh;
  color: var(--ink);
  background: var(--bg);
  font: 14px/1.45 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}
header {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  padding: 16px 20px;
  border-bottom: 1px solid var(--line);
  background: var(--panel);
}
h1 {
  margin: 0;
  font-size: 18px;
  font-weight: 650;
}
.meta {
  color: var(--muted);
  font-size: 12px;
  text-align: right;
}
main {
  display: grid;
  grid-template-columns: 320px minmax(0, 1fr) 380px;
  height: calc(100vh - 67px);
}
aside {
  overflow: auto;
  padding: 14px;
  border-right: 1px solid var(--line);
  background: var(--panel);
}
aside:last-child {
  border-right: 0;
  border-left: 1px solid var(--line);
}
label, .label {
  display: block;
  margin: 10px 0 5px;
  color: var(--muted);
  font-size: 12px;
  font-weight: 650;
  text-transform: uppercase;
}
input[type="search"], input[type="number"] {
  width: 100%;
  min-height: 34px;
  padding: 6px 8px;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: #fff;
  color: var(--ink);
}
.checks {
  display: grid;
  gap: 6px;
}
.checks label, .toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0;
  color: var(--ink);
  font-size: 13px;
  font-weight: 500;
  text-transform: none;
}
button {
  width: 100%;
  min-height: 34px;
  margin-top: 10px;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: #eef2f7;
  color: var(--ink);
  font-weight: 650;
  cursor: pointer;
}
button:hover { background: #e3e9f2; }
.section {
  padding: 12px 0;
  border-top: 1px solid var(--line);
}
.section:first-child { border-top: 0; padding-top: 0; }
.stack {
  display: grid;
  gap: 6px;
}
.row-button {
  width: 100%;
  min-height: 0;
  margin: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 8px;
  align-items: center;
  padding: 7px 9px;
  border-radius: 6px;
  background: #fff;
  font-weight: 500;
  text-align: left;
}
.row-button strong, .row-button span { overflow-wrap: anywhere; }
.row-button.active {
  border-color: var(--accent);
  background: #eff6ff;
}
.result-type, .badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 2px 6px;
  border: 1px solid var(--line);
  border-radius: 999px;
  color: var(--muted);
  font-size: 11px;
  font-weight: 650;
}
.badge.drift {
  border-color: rgba(185, 28, 28, 0.35);
  color: var(--drift);
  background: rgba(185, 28, 28, 0.08);
}
.muted {
  color: var(--muted);
  font-size: 12px;
}
.tour-box {
  padding: 10px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: #fbfcfd;
}
.tour-box h2 {
  margin: 0 0 5px;
  font-size: 14px;
}
.button-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
}
#canvasWrap {
  position: relative;
  min-width: 0;
  min-height: 0;
}
#graphCanvas {
  display: block;
  width: 100%;
  height: 100%;
}
#canvasMeta {
  position: absolute;
  left: 14px;
  bottom: 14px;
  max-width: min(520px, calc(100% - 28px));
  padding: 8px 10px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.9);
  box-shadow: var(--shadow);
  color: var(--muted);
  font-size: 12px;
}
.legend {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 10px;
}
.pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 7px;
  border: 1px solid var(--line);
  border-radius: 999px;
  color: var(--muted);
  font-size: 12px;
}
.dot {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: var(--file);
}
.dot.symbol { background: var(--symbol); }
.dot.concept { background: var(--concept); }
#details h2 {
  margin: 0 0 8px;
  font-size: 16px;
  overflow-wrap: anywhere;
}
.details-section {
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px solid var(--line);
}
.edge-list {
  display: grid;
  gap: 6px;
}
.edge-row {
  width: 100%;
  min-height: 0;
  margin: 0;
  display: grid;
  gap: 3px;
  padding: 8px;
  border-radius: 6px;
  background: #fbfcfd;
  font-weight: 500;
  text-align: left;
}
.edge-row:hover { background: #eef2f7; }
.command {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 58px;
  gap: 8px;
  align-items: start;
  margin-top: 6px;
}
.command code {
  display: block;
  overflow-x: auto;
  padding: 8px;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: #f3f5f7;
  font-size: 12px;
}
.copy {
  min-height: 31px;
  margin: 0;
  padding: 4px 8px;
  font-size: 12px;
}
dl {
  display: grid;
  grid-template-columns: 92px minmax(0, 1fr);
  gap: 6px 10px;
}
dt { color: var(--muted); }
dd {
  margin: 0;
  overflow-wrap: anywhere;
}
pre {
  overflow: auto;
  max-height: 280px;
  padding: 10px;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: #f3f5f7;
  font-size: 12px;
}
@media (max-width: 950px) {
  main {
    grid-template-columns: 1fr;
    height: auto;
  }
  #canvasWrap { height: 58vh; }
  aside, aside:last-child {
    border: 0;
    border-bottom: 1px solid var(--line);
  }
}
</style>
</head>
<body>
<header>
  <div>
    <h1>synrepo graph</h1>
    <div class="legend">
      <span class="pill"><span class="dot"></span>file</span>
      <span class="pill"><span class="dot symbol"></span>symbol</span>
      <span class="pill"><span class="dot concept"></span>concept</span>
      <span class="pill"><span class="dot" style="background:var(--drift)"></span>drift</span>
    </div>
  </div>
  <div class="meta" id="summary"></div>
</header>
<main>
  <aside>
    <div class="section">
      <label for="query">Search</label>
      <input id="query" type="search" placeholder="Path, symbol, id, metadata">
      <div class="label">Results</div>
      <div class="stack" id="searchResults"></div>
    </div>
    <div class="section">
      <div class="label">Node types</div>
      <div class="checks" id="nodeFilters"></div>
      <div class="label">Edge kinds</div>
      <div class="checks" id="edgeFilters"></div>
      <label for="degree">Minimum degree</label>
      <input id="degree" type="number" min="0" value="0">
      <label class="toggle"><input id="driftOnly" type="checkbox"> Show drift/change nodes only</label>
    </div>
    <div class="section">
      <div class="label">Path communities</div>
      <div class="stack" id="groups"></div>
    </div>
    <div class="section">
      <div class="label">Guided walkthrough</div>
      <div class="tour-box" id="tourPanel"></div>
    </div>
    <button id="reset">Reset view</button>
  </aside>
  <section id="canvasWrap">
    <canvas id="graphCanvas"></canvas>
    <div id="canvasMeta"></div>
  </section>
  <aside id="details">
    <h2>No node selected</h2>
    <p>Select a node to inspect graph facts, incident relationships, and card targets.</p>
  </aside>
</main>
<script id="graph-data" type="application/json">"#;
