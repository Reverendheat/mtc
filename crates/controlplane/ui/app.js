const state = {
  nodes: [],
  loading: false,
  launchInFlight: false,
  nodeActionInFlight: new Set(),
};

const elements = {
  nodeGrid: document.querySelector("#node-grid"),
  statusLine: document.querySelector("#status-line"),
  launchForm: document.querySelector("#launch-form"),
  launchButton: document.querySelector("#launch-button"),
  refreshButton: document.querySelector("#refresh-button"),
  nodeIdInput: document.querySelector("#node-id"),
  summaryTotal: document.querySelector("#summary-total"),
  summaryRunning: document.querySelector("#summary-running"),
  summaryPending: document.querySelector("#summary-pending"),
  summaryCordoned: document.querySelector("#summary-cordoned"),
  template: document.querySelector("#node-card-template"),
};

async function fetchNodes() {
  setLoading(true, "Refreshing node inventory...");

  try {
    const response = await fetch("/api/nodes");
    if (!response.ok) {
      throw new Error(`Failed to load nodes (${response.status})`);
    }

    state.nodes = await response.json();
    render();
    updateStatus(`Showing ${state.nodes.length} node${state.nodes.length === 1 ? "" : "s"}.`);
  } catch (error) {
    updateStatus(error.message);
  } finally {
    setLoading(false);
  }
}

async function launchNode(event) {
  event.preventDefault();

  if (state.launchInFlight) {
    return;
  }

  const nodeId = elements.nodeIdInput.value.trim();

  state.launchInFlight = true;
  elements.launchButton.disabled = true;
  updateStatus(nodeId ? `Launching ${nodeId}...` : "Launching node...");

  try {
    const response = await fetch("/api/nodes", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ node_id: nodeId || null }),
    });

    if (!response.ok) {
      const message = await response.text();
      throw new Error(message || `Launch failed (${response.status})`);
    }

    const launched = await response.json();
    elements.nodeIdInput.value = "";
    updateStatus(`Launched ${launched.node_id}. Waiting for registration...`);
    await fetchNodes();
  } catch (error) {
    updateStatus(error.message);
  } finally {
    state.launchInFlight = false;
    elements.launchButton.disabled = false;
  }
}

async function stopNode(node) {
  if (state.nodeActionInFlight.has(node.node_id)) {
    return;
  }

  state.nodeActionInFlight.add(node.node_id);
  renderNodes();
  updateStatus(
    node.backend ? `Stopping ${node.node_id}...` : `Deregistering ${node.node_id}...`,
  );

  try {
    const response = await fetch(`/api/nodes/${encodeURIComponent(node.node_id)}/stop`, {
      method: "POST",
    });

    if (!response.ok) {
      const message = await response.text();
      throw new Error(message || `Node action failed (${response.status})`);
    }

    const result = await response.json();
    updateStatus(result.message);
    await fetchNodes();
  } catch (error) {
    updateStatus(error.message);
  } finally {
    state.nodeActionInFlight.delete(node.node_id);
    renderNodes();
  }
}

function setLoading(isLoading, message = "Loading...") {
  state.loading = isLoading;
  elements.refreshButton.disabled = isLoading;

  if (isLoading) {
    updateStatus(message);
  }
}

function updateStatus(message) {
  elements.statusLine.textContent = message;
}

function render() {
  renderSummary();
  renderNodes();
}

function renderSummary() {
  const running = state.nodes.filter((node) => node.observed_state === "Running").length;
  const pending = state.nodes.filter((node) => node.observed_state === "Pending").length;
  const cordoned = state.nodes.filter((node) => node.cordoned).length;

  elements.summaryTotal.textContent = state.nodes.length;
  elements.summaryRunning.textContent = running;
  elements.summaryPending.textContent = pending;
  elements.summaryCordoned.textContent = cordoned;
}

function renderNodes() {
  elements.nodeGrid.innerHTML = "";

  if (!state.nodes.length) {
    const empty = document.createElement("article");
    empty.className = "panel empty-state";
    empty.textContent = "No worker nodes yet. Launch one to start building the cluster.";
    elements.nodeGrid.appendChild(empty);
    return;
  }

  for (const node of state.nodes) {
    const fragment = elements.template.content.cloneNode(true);
    const card = fragment.querySelector(".node-card");
    const stateChip = fragment.querySelector(".state-chip");

    fragment.querySelector(".node-name").textContent = node.name;
    fragment.querySelector(".node-id").textContent = node.node_id;
    stateChip.textContent = node.observed_state;
    stateChip.dataset.state = node.observed_state;

    fragment.querySelector(".desired-state").textContent = node.desired_state;
    fragment.querySelector(".machine-count").textContent = node.machine_count;
    fragment.querySelector(".app-port").textContent = node.app_port ?? "n/a";
    fragment.querySelector(".process-id").textContent = node.process_id ?? "n/a";
    fragment.querySelector(".backend-pill").textContent = node.backend ?? "manual";

    const cordonPill = fragment.querySelector(".cordon-pill");
    const drainPill = fragment.querySelector(".drain-pill");
    const stopButton = fragment.querySelector(".stop-button");

    if (!node.cordoned) {
      cordonPill.classList.add("is-hidden");
    }

    if (!node.draining) {
      drainPill.classList.add("is-hidden");
    }

    if (node.observed_state === "Pending") {
      card.style.transform = "translateY(-2px)";
    }

    stopButton.textContent = node.backend ? "Stop Worker" : "Deregister Node";
    stopButton.dataset.variant = node.backend ? "stop" : "deregister";
    stopButton.disabled = state.nodeActionInFlight.has(node.node_id);
    stopButton.addEventListener("click", () => {
      stopNode(node);
    });

    elements.nodeGrid.appendChild(fragment);
  }
}

elements.launchForm.addEventListener("submit", launchNode);
elements.refreshButton.addEventListener("click", fetchNodes);

fetchNodes();
setInterval(fetchNodes, 5000);
