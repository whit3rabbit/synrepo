//! Browser script setup, filters, and graph drawing.

pub(super) const HTML_SCRIPT_START: &str = r#"</script>
<script>
const graph = JSON.parse(document.getElementById('graph-data').textContent);
const INITIAL_NODE_LIMIT = 250;
const nodesById = new Map(graph.nodes.map((node) => [node.id, node]));
const incidentEdges = new Map();
const positions = new Map();
let selectedId = null;
let activeGroup = null;
let tourIndex = -1;
let tourFocusIds = new Set();
let visibleIds = new Set(topDegreeNodes().slice(0, INITIAL_NODE_LIMIT).map((node) => node.id));

for (const node of graph.nodes) incidentEdges.set(node.id, []);
for (const edge of graph.edges) {
  if (incidentEdges.has(edge.from)) incidentEdges.get(edge.from).push(edge);
  if (incidentEdges.has(edge.to)) incidentEdges.get(edge.to).push(edge);
}

const canvas = document.getElementById('graphCanvas');
const ctx = canvas.getContext('2d');
const query = document.getElementById('query');
const degree = document.getElementById('degree');
const driftOnly = document.getElementById('driftOnly');
const details = document.getElementById('details');
const canvasMeta = document.getElementById('canvasMeta');
const searchResults = document.getElementById('searchResults');
const groups = buildGroups();
const tourSteps = buildTourSteps();

document.getElementById('summary').textContent =
  `${graph.counts.nodes} nodes, ${graph.counts.edges} edges, ${graph.budget} budget`;

function topDegreeNodes() {
  return [...graph.nodes].sort((a, b) => b.degree.total - a.degree.total || a.label.localeCompare(b.label));
}

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
driftOnly.addEventListener('change', draw);
document.getElementById('reset').addEventListener('click', resetView);
window.addEventListener('resize', draw);
canvas.addEventListener('click', selectCanvasNode);

function checkedValues(containerId) {
  return new Set([...document.querySelectorAll(`#${containerId} input:checked`)].map((input) => input.value));
}

function nodePath(node) {
  return node.path || (node.file_id && nodesById.get(node.file_id)?.path) || '';
}

function communityFor(node) {
  const path = nodePath(node);
  if (!path) return node.type;
  const parts = path.split('/').filter(Boolean);
  if ((parts[0] === 'src' || parts[0] === 'tests') && parts[1]) return `${parts[0]}/${parts[1]}`;
  return parts[0] || node.type;
}

function buildGroups() {
  const counts = new Map();
  for (const node of graph.nodes) {
    const group = communityFor(node);
    counts.set(group, (counts.get(group) || 0) + 1);
  }
  return [...counts.entries()]
    .map(([name, count]) => ({ name, count }))
    .sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
}

function renderGroups() {
  const root = document.getElementById('groups');
  const rows = [{ name: null, label: 'All communities', count: graph.nodes.length }]
    .concat(groups.map((group) => ({ name: group.name, label: group.name, count: group.count })));
  root.innerHTML = rows.map((row) => `
    <button class="row-button ${row.name === activeGroup ? 'active' : ''}" data-group="${escapeAttr(row.name || '')}">
      <strong>${escapeHtml(row.label)}</strong><span>${row.count}</span>
    </button>
  `).join('');
  root.querySelectorAll('button').forEach((button) => {
    button.addEventListener('click', () => {
      activeGroup = button.dataset.group || null;
      visibleIds = activeGroup
        ? new Set(graph.nodes.filter((node) => communityFor(node) === activeGroup).map((node) => node.id))
        : new Set(topDegreeNodes().slice(0, INITIAL_NODE_LIMIT).map((node) => node.id));
      selectedId = null;
      renderDetails(null);
      draw();
    });
  });
}

function edgeHasDrift(edge) {
  return Number(edge.drift_score || 0) > 0;
}

function nodeHasDrift(node) {
  return (incidentEdges.get(node.id) || []).some(edgeHasDrift);
}

function passesStaticFilters(node) {
  const types = checkedValues('nodeFilters');
  const minDegree = Number(degree.value || 0);
  if (!types.has(node.type)) return false;
  if (node.degree.total < minDegree) return false;
  if (activeGroup && communityFor(node) !== activeGroup) return false;
  if (driftOnly.checked && !nodeHasDrift(node)) return false;
  return true;
}

function matchesQuery(node) {
  const needle = query.value.trim().toLowerCase();
  if (!needle) return true;
  const metadata = node.metadata ? JSON.stringify(node.metadata) : '';
  return [node.id, node.label, node.path, node.file_id, node.symbol_kind, node.visibility, metadata]
    .filter(Boolean)
    .some((value) => String(value).toLowerCase().includes(needle));
}

function peersFor(id) {
  return (incidentEdges.get(id) || []).map((edge) => edge.from === id ? edge.to : edge.from);
}

function visibleNodeSet() {
  const needle = query.value.trim();
  let candidates = new Set(visibleIds);
  if (needle) {
    candidates = new Set();
    for (const node of graph.nodes) {
      if (passesStaticFilters(node) && matchesQuery(node)) {
        candidates.add(node.id);
        for (const peer of peersFor(node.id)) candidates.add(peer);
      }
    }
  }
  for (const id of tourFocusIds) {
    candidates.add(id);
    for (const peer of peersFor(id)) candidates.add(peer);
  }
  return new Set([...candidates].filter((id) => {
    const node = nodesById.get(id);
    return node && passesStaticFilters(node) && (needle ? true : matchesQuery(node));
  }));
}

function visibleEdges(nodeSet) {
  const kinds = checkedValues('edgeFilters');
  return graph.edges.filter((edge) => kinds.has(edge.kind) && nodeSet.has(edge.from) && nodeSet.has(edge.to));
}

function draw() {
  renderGroups();
  renderTour();
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
  renderSearchResults(nodes);
  canvasMeta.textContent = `${nodes.length} visible nodes, ${edges.length} visible edges${activeGroup ? `, ${activeGroup}` : ''}`;
  if (!nodes.length) {
    ctx.fillStyle = '#5e676f';
    ctx.fillText('No visible nodes match current filters.', 24 * scale, 32 * scale);
    return;
  }

  const cx = canvas.width / 2;
  const cy = canvas.height / 2;
  const maxRadius = Math.max(90 * scale, Math.min(canvas.width, canvas.height) * 0.43);
  nodes.forEach((node, index) => {
    const selected = node.id === selectedId;
    const focused = tourFocusIds.has(node.id);
    const ring = selected ? 0 : 0.34 + ((index % 4) * 0.18);
    const angle = (index / Math.max(1, nodes.length)) * Math.PI * 2;
    positions.set(node.id, {
      x: selected ? cx : cx + Math.cos(angle) * maxRadius * ring,
      y: selected ? cy : cy + Math.sin(angle) * maxRadius * ring,
      focused,
    });
  });

  ctx.lineWidth = Math.max(1, scale);
  for (const edge of edges) drawEdge(edge, scale);
  for (const node of nodes) drawNode(node, scale);
}

function drawEdge(edge, scale) {
  const from = positions.get(edge.from);
  const to = positions.get(edge.to);
  if (!from || !to) return;
  const selected = edge.from === selectedId || edge.to === selectedId;
  ctx.strokeStyle = edgeColor(edge.kind, edge.drift_score);
  ctx.lineWidth = (selected ? 2 : 1) * scale;
  ctx.beginPath();
  ctx.moveTo(from.x, from.y);
  ctx.lineTo(to.x, to.y);
  ctx.stroke();
  if (selected) {
    ctx.fillStyle = '#3c4650';
    ctx.fillText(edge.kind, (from.x + to.x) / 2, (from.y + to.y) / 2);
  }
}

function drawNode(node, scale) {
  const point = positions.get(node.id);
  const selected = node.id === selectedId;
  const focused = point.focused;
  const radius = (selected ? 9 : focused ? 8 : 6) * scale + Math.min(8, node.degree.total) * 0.45 * scale;
  ctx.fillStyle = nodeColor(node.type);
  ctx.beginPath();
  ctx.arc(point.x, point.y, radius, 0, Math.PI * 2);
  ctx.fill();
  if (selected || focused || nodeHasDrift(node)) {
    ctx.strokeStyle = selected ? '#111827' : nodeHasDrift(node) ? '#b91c1c' : '#2563eb';
    ctx.lineWidth = (selected ? 2 : 1.5) * scale;
    ctx.stroke();
  }
}

function renderSearchResults(visibleNodes) {
  const needle = query.value.trim();
  const matches = (needle ? graph.nodes.filter((node) => passesStaticFilters(node) && matchesQuery(node)) : visibleNodes)
    .sort((a, b) => b.degree.total - a.degree.total || a.label.localeCompare(b.label))
    .slice(0, 10);
  searchResults.innerHTML = matches.length ? matches.map((node) => `
    <button class="row-button" data-node="${escapeAttr(node.id)}">
      <strong>${escapeHtml(node.label)}</strong>
      <span class="result-type">${escapeHtml(node.type)} ${node.degree.total}</span>
    </button>
  `).join('') : '<div class="muted">No matching nodes.</div>';
  searchResults.querySelectorAll('button').forEach((button) => {
    button.addEventListener('click', () => selectNode(button.dataset.node, true));
  });
}

function selectCanvasNode(event) {
  const rect = canvas.getBoundingClientRect();
  const scale = window.devicePixelRatio || 1;
  const x = (event.clientX - rect.left) * scale;
  const y = (event.clientY - rect.top) * scale;
  let nearest = null;
  let nearestDistance = 18 * scale;
  for (const [id, point] of positions.entries()) {
    const distance = Math.hypot(point.x - x, point.y - y);
    if (distance < nearestDistance) {
      nearest = id;
      nearestDistance = distance;
    }
  }
  selectNode(nearest, false);
}

function selectNode(id, expand) {
  selectedId = id || null;
  if (expand && id) {
    visibleIds.add(id);
    for (const peer of peersFor(id)) visibleIds.add(peer);
  }
  renderDetails(id ? nodesById.get(id) : null);
  draw();
}
"#;
