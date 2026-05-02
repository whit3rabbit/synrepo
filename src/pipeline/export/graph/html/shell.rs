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
  --bg: #f8f8f5;
  --panel: #ffffff;
  --ink: #1e2428;
  --muted: #5e676f;
  --line: #d5d9dc;
  --file: #2563eb;
  --symbol: #0f766e;
  --concept: #a16207;
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
  grid-template-columns: 300px minmax(0, 1fr) 340px;
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
.checks label {
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
    </div>
  </div>
  <div class="meta" id="summary"></div>
</header>
<main>
  <aside>
    <label for="query">Search</label>
    <input id="query" type="search" placeholder="Path, symbol, id">
    <div class="label">Node types</div>
    <div class="checks" id="nodeFilters"></div>
    <div class="label">Edge kinds</div>
    <div class="checks" id="edgeFilters"></div>
    <label for="degree">Minimum degree</label>
    <input id="degree" type="number" min="0" value="0">
    <button id="reset">Reset view</button>
  </aside>
  <section id="canvasWrap">
    <canvas id="graphCanvas"></canvas>
  </section>
  <aside id="details">
    <h2>No node selected</h2>
    <p>Click a node to inspect it. Search results include one-hop neighbors. Use expand to add more neighborhood context.</p>
  </aside>
</main>
<script id="graph-data" type="application/json">"#;
