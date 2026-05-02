//! Browser script for the graph export.

pub(super) const HTML_SUFFIX: &str = r#"</script>
<script>
const graph = JSON.parse(document.getElementById('graph-data').textContent);
const INITIAL_NODE_LIMIT = 250;
const nodesById = new Map(graph.nodes.map((node) => [node.id, node]));
const incidentById = new Map();
const positions = new Map();
let selectedId = null;
let visibleIds = new Set(
  [...graph.nodes]
    .sort((a, b) => b.degree.total - a.degree.total || a.label.localeCompare(b.label))
    .slice(0, INITIAL_NODE_LIMIT)
    .map((node) => node.id)
);

for (const node of graph.nodes) incidentById.set(node.id, []);
for (const edge of graph.edges) {
  if (incidentById.has(edge.from)) incidentById.get(edge.from).push(edge.to);
  if (incidentById.has(edge.to)) incidentById.get(edge.to).push(edge.from);
}

const canvas = document.getElementById('graphCanvas');
const ctx = canvas.getContext('2d');
const query = document.getElementById('query');
const degree = document.getElementById('degree');
const details = document.getElementById('details');

document.getElementById('summary').textContent =
  `${graph.counts.nodes} nodes, ${graph.counts.edges} edges, ${graph.budget} budget`;

function unique(values) {
  return [...new Set(values)].sort();
}

function buildCheckboxes(containerId, values) {
  const container = document.getElementById(containerId);
  container.innerHTML = '';
  for (const value of values) {
    const label = document.createElement('label');
    label.innerHTML = `<input type="checkbox" value="${escapeHtml(value)}" checked> ${escapeHtml(value)}`;
    container.appendChild(label);
  }
  container.addEventListener('change', draw);
}

buildCheckboxes('nodeFilters', unique(graph.nodes.map((node) => node.type)));
buildCheckboxes('edgeFilters', unique(graph.edges.map((edge) => edge.kind)));

query.addEventListener('input', draw);
degree.addEventListener('input', draw);
document.getElementById('reset').addEventListener('click', () => {
  query.value = '';
  degree.value = '0';
  selectedId = null;
  visibleIds = new Set(
    [...graph.nodes]
      .sort((a, b) => b.degree.total - a.degree.total || a.label.localeCompare(b.label))
      .slice(0, INITIAL_NODE_LIMIT)
      .map((node) => node.id)
  );
  renderDetails(null);
  draw();
});

window.addEventListener('resize', draw);
canvas.addEventListener('click', (event) => {
  const rect = canvas.getBoundingClientRect();
  const scale = window.devicePixelRatio || 1;
  const x = (event.clientX - rect.left) * scale;
  const y = (event.clientY - rect.top) * scale;
  let nearest = null;
  let nearestDistance = 18 * scale;
  for (const [id, point] of positions.entries()) {
    const dx = point.x - x;
    const dy = point.y - y;
    const distance = Math.sqrt(dx * dx + dy * dy);
    if (distance < nearestDistance) {
      nearest = id;
      nearestDistance = distance;
    }
  }
  selectedId = nearest;
  renderDetails(nearest ? nodesById.get(nearest) : null);
  draw();
});

function checkedValues(containerId) {
  return new Set(
    [...document.querySelectorAll(`#${containerId} input:checked`)].map((input) => input.value)
  );
}

function nodeMatches(node) {
  const types = checkedValues('nodeFilters');
  const minDegree = Number(degree.value || 0);
  if (!types.has(node.type)) return false;
  if (node.degree.total < minDegree) return false;
  const needle = query.value.trim().toLowerCase();
  if (!needle) return true;
  return [node.id, node.label, node.path, node.file_id, node.symbol_kind, node.visibility]
    .filter(Boolean)
    .some((value) => String(value).toLowerCase().includes(needle));
}

function visibleNodeSet() {
  const needle = query.value.trim();
  let candidates = new Set(visibleIds);
  if (needle) {
    candidates = new Set();
    for (const node of graph.nodes) {
      if (nodeMatches(node)) {
        candidates.add(node.id);
        for (const peer of incidentById.get(node.id) || []) candidates.add(peer);
      }
    }
  }
  return new Set([...candidates].filter((id) => {
    const node = nodesById.get(id);
    return node && nodeMatches(node);
  }));
}

function visibleEdges(nodeSet) {
  const kinds = checkedValues('edgeFilters');
  return graph.edges.filter((edge) =>
    kinds.has(edge.kind) && nodeSet.has(edge.from) && nodeSet.has(edge.to)
  );
}

function draw() {
  const scale = window.devicePixelRatio || 1;
  const rect = canvas.parentElement.getBoundingClientRect();
  canvas.width = Math.max(320, Math.floor(rect.width * scale));
  canvas.height = Math.max(320, Math.floor(rect.height * scale));
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  positions.clear();

  const nodeSet = visibleNodeSet();
  const nodes = [...nodeSet].map((id) => nodesById.get(id)).filter(Boolean)
    .sort((a, b) => b.degree.total - a.degree.total || a.label.localeCompare(b.label));
  const edges = visibleEdges(nodeSet);
  if (!nodes.length) {
    ctx.fillStyle = '#5e676f';
    ctx.fillText('No visible nodes match current filters.', 24 * scale, 32 * scale);
    return;
  }

  const cx = canvas.width / 2;
  const cy = canvas.height / 2;
  const maxRadius = Math.max(90 * scale, Math.min(canvas.width, canvas.height) * 0.43);
  nodes.forEach((node, index) => {
    const ring = 0.35 + ((index % 4) * 0.18);
    const angle = (index / Math.max(1, nodes.length)) * Math.PI * 2;
    positions.set(node.id, {
      x: cx + Math.cos(angle) * maxRadius * ring,
      y: cy + Math.sin(angle) * maxRadius * ring,
    });
  });

  ctx.lineWidth = Math.max(1, scale);
  for (const edge of edges) {
    const from = positions.get(edge.from);
    const to = positions.get(edge.to);
    if (!from || !to) continue;
    ctx.strokeStyle = edgeColor(edge.kind, edge.drift_score);
    ctx.beginPath();
    ctx.moveTo(from.x, from.y);
    ctx.lineTo(to.x, to.y);
    ctx.stroke();
  }

  for (const node of nodes) {
    const point = positions.get(node.id);
    const radius = (node.id === selectedId ? 8 : 6) * scale + Math.min(8, node.degree.total) * 0.45 * scale;
    ctx.fillStyle = nodeColor(node.type);
    ctx.beginPath();
    ctx.arc(point.x, point.y, radius, 0, Math.PI * 2);
    ctx.fill();
    if (node.id === selectedId) {
      ctx.strokeStyle = '#111827';
      ctx.lineWidth = 2 * scale;
      ctx.stroke();
    }
  }
}

function nodeColor(type) {
  if (type === 'symbol') return '#0f766e';
  if (type === 'concept') return '#a16207';
  return '#2563eb';
}

function edgeColor(kind, drift) {
  if (drift && drift > 0.7) return '#b91c1c';
  if (kind === 'calls') return '#64748b';
  if (kind === 'defines') return '#94a3b8';
  if (kind === 'imports') return '#475569';
  return '#78716c';
}

function renderDetails(node) {
  if (!node) {
    details.innerHTML = '<h2>No node selected</h2><p>Click a node to inspect it. Search results include one-hop neighbors. Use expand to add more neighborhood context.</p>';
    return;
  }
  const metadata = JSON.stringify(node.metadata || {}, null, 2);
  details.innerHTML = `
    <h2>${escapeHtml(node.label)}</h2>
    <dl>
      <dt>ID</dt><dd>${escapeHtml(node.id)}</dd>
      <dt>Type</dt><dd>${escapeHtml(node.type)}</dd>
      <dt>Degree</dt><dd>${node.degree.total} (${node.degree.inbound} in, ${node.degree.outbound} out)</dd>
      <dt>Path</dt><dd>${escapeHtml(node.path || '')}</dd>
      <dt>Kind</dt><dd>${escapeHtml(node.symbol_kind || '')}</dd>
      <dt>Visibility</dt><dd>${escapeHtml(node.visibility || '')}</dd>
    </dl>
    <button id="expand">Expand neighborhood</button>
    <div class="label">Metadata</div>
    <pre>${escapeHtml(metadata)}</pre>
  `;
  document.getElementById('expand').addEventListener('click', () => {
    visibleIds.add(node.id);
    for (const peer of incidentById.get(node.id) || []) visibleIds.add(peer);
    draw();
  });
}

function escapeHtml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

draw();
</script>
</body>
</html>
"#;
