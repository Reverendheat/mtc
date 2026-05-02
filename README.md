# MTC

**MTC** is a tiny machine orchestration experiment.

The goal is to model a small control plane and a fleet of worker nodes that can launch and track lightweight “machines” across a cluster. Right now it is intentionally simple and in-memory, with a focus on getting the control flow and system shape right before adding persistence, process supervision, or more realistic scheduling.

## Current idea

There are three conceptual pieces:

- **`mtc`** — a short-lived CLI for issuing commands to the control plane
- **control plane** — a long-lived service that tracks cluster state
- **workers** — long-lived node agents that register, heartbeat, and run assigned workloads

A worker represents a node in the cluster.

A machine represents a launched workload assigned to a node.

Machines can carry a shell command. The control plane stores the assignment, the target worker polls for pending work, claims it, runs it locally, and reports back the final state, exit code, stdout, and stderr.

Workers explicitly advertise whether they support machine execution when they register. The scheduler only assigns command machines to nodes that report that capability, which prevents older heartbeat-only workers from accepting work they cannot run.

## Current status

This project is very much a work in progress.

Things being explored right now:

- Rust workspace layout
- shared types in a `common` crate
- worker registration
- worker heartbeats
- in-memory node and machine state
- node cordon / drain lifecycle
- local worker launching from the control plane
- one-shot command execution on worker nodes
- basic CLI calls into the control plane
- a tiny buildless SPA for node and machine visibility

Things that are **not** done yet:

- durable state
- machine reconciliation
- real cancellation for running machines
- scheduling beyond simple/random assignment
- robust error handling
- polished CLI output

## CLI

The `mtc` CLI talks to the control plane over HTTP. By default it targets `http://127.0.0.1:3000`.

You can override the control plane URL with either a flag:

```bash
mtc --control-plane-url http://127.0.0.1:3000 status
```

or an environment variable:

```bash
MTC_CONTROL_PLANE_URL=http://127.0.0.1:3000 mtc status
```

Launch a one-shot command machine:

```bash
mtc launch hello --command "printf ok"
```

Launch on a specific node:

```bash
mtc launch hello --node-id worker-node-1 --command "printf ok"
```

List machines:

```bash
mtc status
```

Show one machine in detail:

```bash
mtc status --machine-id <machine-id>
```

Remove a machine record from the control plane:

```bash
mtc stop --machine-id <machine-id>
```

Note: `stop` currently removes the machine record from the in-memory control plane. It does not cancel or kill a command that a worker has already claimed and started running.

## Node lifecycle

Workers can be marked as:

- `cordoned` — the node stays registered and heartbeating, but it will not receive new machine assignments
- `draining` — the node is cordoned and waiting for its existing machine assignments to reach zero

In the current in-memory implementation:

- the scheduler only places new machines on running nodes that are neither cordoned nor draining
- heartbeats refresh node liveness without clearing `cordoned` or `draining`
- when the last machine is removed from a draining node, the node stays cordoned and exits draining

Each node also tracks:

- `observed_state` — what the control plane currently believes the node is doing based on heartbeats and reaping
- `desired_state` — what the control plane wants the node state to be

## Worker launching

The control plane has a launcher abstraction with a local-process backend.

- `POST /workers/launch` starts a real `mtcworker` process
- `POST /api/nodes` launches a new worker node for the SPA/API
- the control plane creates a placeholder node with `observed_state=Pending` and `desired_state=Running`
- the worker still becomes fully active by calling the existing registration endpoint
- Docker Compose runs only the control plane; worker instances are expected to be launched by the control plane itself

The local launcher currently passes instance-specific environment to each worker:

- `NODE_ID`
- `CONTROL_PLANE_URL`

`CONTROL_PLANE_URL` is derived automatically from the control plane's own `APP_PORT` for the local launcher.

By default, the control plane looks for the worker binary next to the control plane binary. You can override that with `WORKER_BINARY_PATH`.

## SPA

The control plane serves a tiny buildless SPA.

- `GET /` serves the node dashboard
- `GET /api/nodes` returns the current node list with launch metadata when available
- `POST /api/nodes` launches a new worker node for the SPA/API
- `GET /api/machines` returns stored machine runs with command output
- `POST /api/machines` launches a one-shot command machine from the SPA/API

The frontend lives in plain files under `crates/controlplane/ui/index.html`, `crates/controlplane/ui/styles.css`, and `crates/controlplane/ui/app.js`.

## Runtime note

Commands currently run directly on the worker host via the platform shell:

- Unix-like systems use `sh -lc`
- Windows uses `cmd /C`

That keeps the runtime tiny while we validate the end-to-end control flow. If we want stronger isolation next, Docker is a natural follow-up backend behind the same machine execution boundary.

## Workspace layout

```text
crates/
  common/
  cmd/
  controlplane/
  worker/
```
