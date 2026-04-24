const state = {
  nodes: [],
  machines: [],
  loading: false,
  launchInFlight: false,
  machineLaunchInFlight: false,
  nodeActionInFlight: new Set(),
  statusMessage: "Loading nodes...",
  statusTone: "info",
};

const elements = {
  nodeGrid: document.querySelector("#node-grid"),
  machineGrid: document.querySelector("#machine-grid"),
  statusLine: document.querySelector("#status-line"),
  launchForm: document.querySelector("#launch-form"),
  launchButton: document.querySelector("#launch-button"),
  machineForm: document.querySelector("#machine-form"),
  machineButton: document.querySelector("#machine-button"),
  machineNameInput: document.querySelector("#machine-name"),
  machineCommandInput: document.querySelector("#machine-command"),
  machineNodeIdInput: document.querySelector("#machine-node-id"),
  refreshButton: document.querySelector("#refresh-button"),
  nodeIdInput: document.querySelector("#node-id"),
  summaryTotal: document.querySelector("#summary-total"),
  summaryRunning: document.querySelector("#summary-running"),
  summaryPending: document.querySelector("#summary-pending"),
  summaryCordoned: document.querySelector("#summary-cordoned"),
  template: document.querySelector("#node-card-template"),
  machineTemplate: document.querySelector("#machine-card-template"),
};

async function fetchData() {
  setLoading(true, "Refreshing node inventory...");

  try {
    const [nodeResponse, machineResponse] = await Promise.all([
      fetch("/api/nodes"),
      fetch("/api/machines"),
    ]);

    if (!nodeResponse.ok) {
      throw new Error(`Failed to load nodes (${nodeResponse.status})`);
    }

    if (!machineResponse.ok) {
      throw new Error(`Failed to load machines (${machineResponse.status})`);
    }

    state.nodes = await nodeResponse.json();
    state.machines = await machineResponse.json();
    render();
    if (state.statusTone !== "error") {
      updateStatus(
        `Showing ${state.nodes.length} node${state.nodes.length === 1 ? "" : "s"}.`,
      );
    }
  } catch (error) {
    updateStatus(error.message, "error");
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
    await fetchData();
  } catch (error) {
    updateStatus(error.message, "error");
  } finally {
    state.launchInFlight = false;
    elements.launchButton.disabled = false;
  }
}

async function launchMachine(event) {
  event.preventDefault();

  if (state.machineLaunchInFlight) {
    return;
  }

  const machineName = elements.machineNameInput.value.trim();
  const command = elements.machineCommandInput.value.trim();
  const nodeId = elements.machineNodeIdInput.value.trim();

  state.machineLaunchInFlight = true;
  elements.machineButton.disabled = true;
  updateStatus(`Launching machine ${machineName || "job"}...`);

  try {
    const response = await fetch("/api/machines", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        machine_name: machineName,
        command,
        node_id: nodeId || null,
      }),
    });

    if (!response.ok) {
      const message = await response.text();
      throw new Error(message || `Machine launch failed (${response.status})`);
    }

    const launched = await response.json();
    elements.machineNameInput.value = "";
    elements.machineCommandInput.value = "";
    elements.machineNodeIdInput.value = "";
    updateStatus(`Queued ${launched.machine_name} on ${launched.node_id}.`);
    await fetchData();
  } catch (error) {
    updateStatus(error.message, "error");
  } finally {
    state.machineLaunchInFlight = false;
    syncMachineLaunchAvailability();
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
    await fetchData();
  } catch (error) {
    updateStatus(error.message, "error");
  } finally {
    state.nodeActionInFlight.delete(node.node_id);
    renderNodes();
  }
}

function setLoading(isLoading, message = "Loading...") {
  state.loading = isLoading;
  elements.refreshButton.disabled = isLoading;

  if (isLoading && state.statusTone !== "error") {
    updateStatus(message);
  }
}

function updateStatus(message, tone = "info") {
  state.statusMessage = message;
  state.statusTone = tone;
  elements.statusLine.textContent = message;
  elements.statusLine.dataset.tone = tone;
}

function render() {
  renderSummary();
  renderNodes();
  renderMachines();
  syncMachineLaunchAvailability();
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
    fragment.querySelector(".execution-support").textContent = node.supports_machine_execution
      ? "ready"
      : "legacy";
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

function renderMachines() {
  elements.machineGrid.innerHTML = "";

  if (!state.machines.length) {
    const empty = document.createElement("article");
    empty.className = "panel empty-state";
    empty.textContent = "No machines yet. Launch a command to watch a worker pick it up.";
    elements.machineGrid.appendChild(empty);
    return;
  }

  for (const machine of state.machines) {
    const fragment = elements.machineTemplate.content.cloneNode(true);
    const stateChip = fragment.querySelector(".machine-state");

    fragment.querySelector(".machine-name").textContent = machine.machine_name;
    fragment.querySelector(".machine-id").textContent = machine.machine_id;
    fragment.querySelector(".machine-node").textContent = machine.node_id;
    fragment.querySelector(".machine-exit-code").textContent = machine.exit_code ?? "n/a";
    fragment.querySelector(".machine-command").textContent = machine.command;
    fragment.querySelector(".machine-stdout").textContent = machine.stdout || "No stdout";
    fragment.querySelector(".machine-stderr").textContent = machine.stderr || "No stderr";

    stateChip.textContent = machine.state;
    stateChip.dataset.state = machine.state;

    elements.machineGrid.appendChild(fragment);
  }
}

function syncMachineLaunchAvailability() {
  const runnableNodes = state.nodes.filter(
    (node) =>
      node.observed_state === "Running" &&
      node.desired_state === "Running" &&
      node.supports_machine_execution &&
      !node.cordoned &&
      !node.draining,
  );

  const noRunnableNodes = runnableNodes.length === 0;

  elements.machineButton.disabled = state.machineLaunchInFlight;
  elements.machineButton.textContent = state.machineLaunchInFlight
    ? "Launching..."
    : "Launch Machine";
  elements.machineButton.dataset.blocked = noRunnableNodes ? "true" : "false";

  if (!state.machineLaunchInFlight && noRunnableNodes) {
    elements.machineButton.title = "No runnable nodes are available yet. Launch or restart a ready worker first.";
  } else {
    elements.machineButton.title = "";
  }
}

elements.launchForm.addEventListener("submit", launchNode);
elements.machineForm.addEventListener("submit", launchMachine);
elements.refreshButton.addEventListener("click", fetchData);

fetchData();
setInterval(fetchData, 5000);
