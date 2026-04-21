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
- basic CLI shape

Things that are **not** done yet:

- real process launching
- durable state
- machine reconciliation
- cordon / drain behavior
- scheduling beyond simple/random assignment
- robust error handling
- polished CLI output

## Workspace layout

```text
crates/
  common/
  ctl/
  controlplane/
  worker/
