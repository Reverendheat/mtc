# MTC

**MTC** is a tiny machine orchestration experiment.

The goal is to model a small control plane and a fleet of worker nodes that can launch and track lightweight “machines” across a cluster. Right now it is intentionally simple and in-memory, with a focus on getting the control flow and system shape right before adding persistence, process supervision, or more realistic scheduling.

## Current idea

There are three conceptual pieces:

- **`mtc`** — a short-lived CLI for issuing commands
- **control plane** — a long-lived service that tracks cluster state
- **workers** — long-lived node agents that register, heartbeat, and eventually run assigned workloads

A worker represents a node in the cluster.

A machine represents a launched workload assigned to a node.

## Current status

This project is very much a work in progress.

Things being explored right now:

- Rust workspace layout
- shared types in a `common` crate
- worker registration
- worker heartbeats
- in-memory node and machine state
- node cordon / drain lifecycle
- basic CLI shape

Things that are **not** done yet:

- real process launching
- durable state
- machine reconciliation
- scheduling beyond simple/random assignment
- robust error handling
- polished CLI output

## Node lifecycle

Workers can now be marked as:

- `cordoned` — the node stays registered and heartbeating, but it will not receive new machine assignments
- `draining` — the node is cordoned and waiting for its existing machine assignments to reach zero

In the current in-memory implementation:

- the scheduler only places new machines on running nodes that are neither cordoned nor draining
- heartbeats refresh node liveness without clearing `cordoned` or `draining`
- when the last machine is removed from a draining node, the node stays cordoned and exits draining

## Workspace layout

```text
crates/
  common/
  ctl/
  controlplane/
  worker/
