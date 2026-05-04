//! Browser script tour, details, and utility helpers.

pub(super) const HTML_SCRIPT_END: &str = r#"function buildTourSteps() {
  const files = topDegreeNodes().filter((node) => node.type === 'file').slice(0, 6).map((node) => node.id);
  const symbols = topDegreeNodes().filter((node) => node.type === 'symbol').slice(0, 6).map((node) => node.id);
  const drift = topDegreeNodes().filter(nodeHasDrift).slice(0, 6).map((node) => node.id);
  const concepts = topDegreeNodes().filter((node) => node.type === 'concept').slice(0, 6).map((node) => node.id);
  const steps = [];
  if (files.length) steps.push({ title: 'High-degree files', body: 'Start with files that own the busiest structural relationships.', ids: files });
  if (symbols.length) steps.push({ title: 'Connected symbols', body: 'Follow symbols with the most graph neighbors, then open their cards for detail.', ids: symbols });
  if (drift.length) steps.push({ title: 'Drift and change hotspots', body: 'Inspect nodes touching scored drift edges before editing nearby code.', ids: drift });
  if (concepts.length) steps.push({ title: 'Human-declared concepts', body: 'Read declared concepts that govern or explain graph facts.', ids: concepts });
  if (!steps.length) steps.push({ title: 'Top graph nodes', body: 'Review the most connected nodes in this export.', ids: topDegreeNodes().slice(0, 6).map((node) => node.id) });
  return steps;
}

function renderTour() {
  const panel = document.getElementById('tourPanel');
  if (!tourSteps.length) {
    panel.innerHTML = '<div class="muted">No walkthrough steps available.</div>';
    return;
  }
  const active = tourIndex >= 0 ? tourSteps[tourIndex] : null;
  panel.innerHTML = active ? `
    <h2>${escapeHtml(active.title)}</h2>
    <p class="muted">${escapeHtml(active.body)}</p>
    <p class="muted">${tourIndex + 1} of ${tourSteps.length}, ${active.ids.length} highlighted nodes</p>
    <div class="button-row">
      <button id="tourPrev">Previous</button>
      <button id="tourNext">${tourIndex + 1 === tourSteps.length ? 'Finish' : 'Next'}</button>
    </div>
    <button id="tourStop">Stop walkthrough</button>
  ` : `
    <p class="muted">${tourSteps.length} deterministic steps from graph degree, drift, and node type.</p>
    <button id="tourStart">Start walkthrough</button>
  `;
  const start = document.getElementById('tourStart');
  const prev = document.getElementById('tourPrev');
  const next = document.getElementById('tourNext');
  const stop = document.getElementById('tourStop');
  if (start) start.addEventListener('click', () => setTourStep(0));
  if (prev) prev.addEventListener('click', () => setTourStep(Math.max(0, tourIndex - 1)));
  if (next) next.addEventListener('click', () => tourIndex + 1 === tourSteps.length ? stopTour() : setTourStep(tourIndex + 1));
  if (stop) stop.addEventListener('click', stopTour);
}

function setTourStep(index) {
  tourIndex = index;
  tourFocusIds = new Set(tourSteps[index].ids);
  for (const id of tourFocusIds) {
    visibleIds.add(id);
    for (const peer of peersFor(id)) visibleIds.add(peer);
  }
  selectedId = tourSteps[index].ids[0] || null;
  renderDetails(selectedId ? nodesById.get(selectedId) : null);
  draw();
}

function stopTour() {
  tourIndex = -1;
  tourFocusIds = new Set();
  draw();
}

function renderDetails(node) {
  if (!node) {
    details.innerHTML = '<h2>No node selected</h2><p>Select a node to inspect graph facts, incident relationships, and card targets.</p>';
    return;
  }
  const metadata = JSON.stringify(node.metadata || {}, null, 2);
  const driftBadge = nodeHasDrift(node) ? '<span class="badge drift">drift</span>' : '';
  details.innerHTML = `
    <h2>${escapeHtml(node.label)}</h2>
    <div>${driftBadge}</div>
    <dl>
      <dt>ID</dt><dd>${escapeHtml(node.id)}</dd>
      <dt>Type</dt><dd>${escapeHtml(node.type)}</dd>
      <dt>Degree</dt><dd>${node.degree.total} (${node.degree.inbound} in, ${node.degree.outbound} out)</dd>
      <dt>Community</dt><dd>${escapeHtml(communityFor(node))}</dd>
      <dt>Path</dt><dd>${escapeHtml(nodePath(node))}</dd>
      <dt>Kind</dt><dd>${escapeHtml(node.symbol_kind || '')}</dd>
      <dt>Visibility</dt><dd>${escapeHtml(node.visibility || '')}</dd>
      <dt>Epistemic</dt><dd>${escapeHtml(node.epistemic || '')}</dd>
    </dl>
    <button id="expand">Expand neighborhood</button>
    <div class="details-section"><div class="label">Card targets</div>${commandHtml(node)}</div>
    <div class="details-section"><div class="label">Incident relationships</div><div class="edge-list">${incidentHtml(node)}</div></div>
    <div class="details-section"><div class="label">Metadata</div><pre>${escapeHtml(metadata)}</pre></div>
  `;
  document.getElementById('expand').addEventListener('click', () => selectNode(node.id, true));
  details.querySelectorAll('[data-node]').forEach((button) => {
    button.addEventListener('click', () => selectNode(button.dataset.node, true));
  });
  details.querySelectorAll('[data-copy]').forEach((button) => {
    button.addEventListener('click', () => copyCommand(button));
  });
}

function commandHtml(node) {
  const target = node.id;
  const kind = node.type === 'file' ? 'file' : node.type === 'symbol' ? 'symbol' : 'search';
  const packTarget = kind === 'search'
    ? JSON.stringify([{ kind, target: node.label, budget: 'tiny' }])
    : JSON.stringify([{ kind, target, budget: 'normal' }]);
  const commands = [
    `synrepo_card target="${target}" budget="deep"`,
    `synrepo_minimum_context target="${target}" budget="normal"`,
    `synrepo_context_pack targets=${packTarget}`,
  ];
  return commands.map((command) => `
    <div class="command">
      <code>${escapeHtml(command)}</code>
      <button class="copy" data-copy="${escapeAttr(command)}">Copy</button>
    </div>
  `).join('');
}

function incidentHtml(node) {
  const rows = [...(incidentEdges.get(node.id) || [])]
    .sort((a, b) => edgeHasDrift(b) - edgeHasDrift(a) || a.kind.localeCompare(b.kind))
    .slice(0, 24);
  if (!rows.length) return '<div class="muted">No incident edges in this export.</div>';
  return rows.map((edge) => {
    const outgoing = edge.from === node.id;
    const peerId = outgoing ? edge.to : edge.from;
    const peer = nodesById.get(peerId);
    const drift = edgeHasDrift(edge) ? `, drift ${Number(edge.drift_score).toFixed(2)}` : '';
    return `
      <button class="edge-row" data-node="${escapeAttr(peerId)}">
        <span>${outgoing ? 'outbound' : 'inbound'} ${escapeHtml(edge.kind)}${escapeHtml(drift)}</span>
        <span class="muted">${escapeHtml(peer?.label || peerId)}</span>
      </button>
    `;
  }).join('');
}

function copyCommand(button) {
  const command = button.dataset.copy || '';
  if (navigator.clipboard?.writeText) {
    navigator.clipboard.writeText(command).then(() => button.textContent = 'Copied').catch(() => button.textContent = 'Copy');
  }
}

function resetView() {
  query.value = '';
  degree.value = '0';
  driftOnly.checked = false;
  selectedId = null;
  activeGroup = null;
  tourIndex = -1;
  tourFocusIds = new Set();
  visibleIds = new Set(topDegreeNodes().slice(0, INITIAL_NODE_LIMIT).map((node) => node.id));
  renderDetails(null);
  draw();
}

function nodeColor(type) {
  if (type === 'symbol') return '#0f766e';
  if (type === 'concept') return '#a16207';
  return '#2563eb';
}

function edgeColor(kind, drift) {
  if (edgeHasDrift({ drift_score: drift })) return '#b91c1c';
  if (kind === 'calls') return '#64748b';
  if (kind === 'defines') return '#94a3b8';
  if (kind === 'imports') return '#475569';
  if (kind === 'governs') return '#a16207';
  return '#78716c';
}

function escapeHtml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

function escapeAttr(value) {
  return escapeHtml(value);
}

draw();
</script>
</body>
</html>
"#;
