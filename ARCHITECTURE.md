# Architecture — HCOM Fork

**Status:** Current truth. Confirmed by Bernardo on 2026-08-08.
**Update rule:** doc-first — changes here commit before the code that needs
them, with the rationale in the commit message.

**How to use:** before any non-trivial change, answer *where does this go?*
from the Routing Table and *am I allowed to decide this?* from the Autonomy
Map. An unclear answer stops for the owner.

## The Product

This repository is Bernardo's maintained fork of HCOM, the upstream Rust CLI
that lets supported coding-agent harnesses message, observe, and launch one
another locally and across trusted devices. It preserves the upstream product
shape: one cross-platform binary, local SQLite state, harness hooks and PTY
integration, plus an encrypted MQTT relay implemented inside the binary.

The immediate operating profile is coordination between Bernardo's Linux and
Windows machines through a private Mosquitto broker. The fork may carry a
bounded repair before it is accepted upstream, but it does not become a new
agent platform, broker, MCP server, or independent protocol.

## The Core Invariant

A cross-device relay may report a state publication as broker-confirmed and
advance its durable event cursor only when the complete MQTT PUBLISH packet is
within the relay compatibility ceiling and the broker has acknowledged that
exact publication. A publication that is oversized, disconnected, timed out,
or otherwise unconfirmed remains retryable and is reported as an error rather
than as success. PUBACK is necessary but does not prove that a peer decoded or
acted on the publication, so user-visible wording must not call it peer
delivery. This rule exists because HCOM 0.7.24 reported `sent` and `connected`
while a real Windows-to-Linux implementation exchange was not delivered.

## Cross-Cutting Rules

1. MQTT enqueue, broker acknowledgement, peer receipt, and agent handling are
   distinct states; code and user-visible status must not collapse them.
2. The repair must introduce `MAX_RELAY_PACKET_BYTES` as the canonical 128 KiB
   compatibility ceiling of the deployed HCOM 0.7.24 peers in this environment.
   Client inbound setup, outgoing packet construction, and tests must use that
   one value. Event batching must keep the complete MQTT PUBLISH packet —
   framing, topic, properties, and sealed payload — within it. No dynamic
   peer-limit discovery is introduced by this repair.
3. If even one state/event unit cannot fit the packet budget, fail loudly and
   leave its cursor unchanged; never truncate, omit, or invent replicated data.
4. QoS 1 cursor advancement must be tied to the PUBACK for the corresponding
   state publication, with a bounded timeout and explicit connection-error
   result. This is necessary but not sufficient for peer receipt; only receiver
   evidence can prove the peer decoded the publication.
5. Database reset must stop owned processes and release every in-process SQLite
   handle before archiving or deleting the database and WAL/SHM files.
6. This fork preserves HCOM's documented full-trust relay security model.
   Transport repair must not add or broaden remote-control authority. Only
   fully trusted devices may enroll, preferably through a private broker.
7. The current repair adds no dependency, service, schema, protocol version,
   MCP surface, or generic remote-execution path.

## Routing Table — Where Does It Go?

| You are adding or changing… | It belongs in… | Never in… |
|---|---|---|
| CLI parsing and command dispatch | `src/main.rs`, `src/router.rs`, command entrypoints in `src/commands/` | relay transport or harness hooks |
| SQLite schema and shared database operations | `src/db/`, `src/paths.rs`, `src/commands/reset_ops.rs` | TUI, MQTT transport, or tool adapters |
| MQTT connection, relay cursors, push/pull, encryption, replay, and remote RPC | `src/relay/`, `src/commands/relay.rs` | CLI presentation or harness-specific hooks |
| Agent identity, lifecycle, launch, and local delivery | `src/identity.rs`, `src/instance_*`, `src/launcher.rs`, `src/delivery*` | relay serialization or TUI rendering |
| Harness integration and transcripts | `src/hooks/`, `src/tools/`, `src/transcript/`, plugin sources | relay transport or database migrations |
| Terminal, PTY, and platform process behavior | `src/terminal.rs`, `src/pty/`, `src/sys/`, `src/pidtrack.rs` | relay state, TUI, or database ownership |
| Dashboard, configuration, update, and user guidance | `src/tui/`, `src/config.rs`, `src/update.rs`, `README.md`, `skills/`, plugin manifests | relay health derivation or test fixtures as authority |
| Cross-platform and real-boundary proof | `tests/`, `scripts/`, `Justfile`, `.github/workflows/`; inline unit tests stay beside private code under `src/` | production behavior |

## Required System Shape / Call Flow

1. A harness is launched or registered through `src/launcher.rs`,
   `src/commands/start.rs`, and harness-specific hooks.
2. Commands enter through `src/main.rs` and `src/router.rs`; message and
   lifecycle handlers record state in the SQLite event/state planes under
   `src/db/`.
3. Local delivery in `src/delivery.rs` and `src/notify/` injects or wakes local
   agents. Terminal and TUI blocks expose the same stored state.
4. When relay is enabled, `src/relay/worker.rs` owns the relay subprocess and
   `src/relay/client.rs` owns the persistent MQTT connection/event loop.
5. Required after the active repair: `src/relay/push.rs` builds an encrypted
   retained state snapshot plus a packet-budgeted event batch. The client
   publishes it at QoS 1 and only a matching PUBACK permits the durable relay
   cursor and broker-confirmed status to advance.
6. `src/relay/pull.rs` authenticates, replay-checks, and applies remote state;
   `src/relay/control.rs` owns the existing full-trust remote RPC contract.
7. Required after the active repair: reset dispatch coordinates process
   shutdown and transfers database file lifecycle to
   `src/commands/reset_ops.rs` only after the router-owned SQLite handle is
   released.

## Blocks

### 1. Application and Command Surface
- **Responsibility:** own shared application primitives, parse CLI intent, establish identity context, and route command entrypoints to one domain owner.
- **NOT responsibility:** implement relay delivery, database file lifecycle, terminal processes, or harness behavior; Blocks 3, 2, 6, and 5 own those.
- **Canonical modules:** `src/main.rs`, `src/router.rs`, `src/cli_context.rs`, `src/bootstrap.rs`, `src/core/`, `src/shared/` except the exact Block 6 files named below, `src/messages.rs`, `src/log.rs`, `src/integration_spec.rs`, and command entrypoints in `src/commands/` except the exact domain-owned files named below.
- **Enforcement:** CLI smoke/unit tests and command exit codes.

### 2. Local State
- **Responsibility:** own SQLite schema, events, instances, shared database access, bindings, and safe database file operations.
- **NOT responsibility:** decide relay cursor or delivery semantics or render state; Blocks 3 and 7 own those.
- **Canonical modules:** `src/db/`, `src/commands/reset_ops.rs`, `src/commands/archive.rs`, `src/paths.rs`.
- **Enforcement:** database unit tests plus Windows reset runtime proof.

### 3. Cross-Device Relay
- **Responsibility:** own MQTT connection lifecycle, bounded encrypted envelopes, acknowledged publication, pull/replay, health, and existing remote RPC transport.
- **NOT responsibility:** execute agent lifecycle operations or redefine their authority; Blocks 4 and the documented security model own those.
- **Canonical modules:** `src/relay/`, `src/commands/relay.rs`.
- **Enforcement:** relay unit tests, real MQTT roundtrip, and cross-device load proof.

### 4. Agent Runtime
- **Responsibility:** own agent identity, launch/resume/kill intent, local message delivery, and notification.
- **NOT responsibility:** implement terminal/PTY processes, replicate state across devices, or own SQLite schema; Blocks 6, 3, and 2 own those.
- **Canonical modules:** `src/identity.rs`, `src/instance_*`, `src/instances.rs`, `src/launcher.rs`, `src/delivery*`, `src/notify/`.
- **Enforcement:** lifecycle/delivery unit tests and pinned real-tool tests.

### 5. Harness Adapters
- **Responsibility:** integrate supported harness hooks, prompts, plugins, and transcripts with the common runtime.
- **NOT responsibility:** create parallel identity, transport, or persistence models; Blocks 4, 3, and 2 own those.
- **Canonical modules:** `src/hooks/`, `src/tools/`, `src/transcript/`, `src/*_plugin/`, `src/claude_actor.rs`, `src/tool.rs`.
- **Enforcement:** typecheck, adapter unit tests, and pinned real-tool tests.

### 6. Terminal and Platform Runtime
- **Responsibility:** own terminal launch/close, PTY behavior, OS process control, runtime environment, and platform primitives.
- **NOT responsibility:** decide agent lifecycle intent, relay state, or dashboard rendering; Blocks 4, 3, and 7 own those.
- **Canonical modules:** `src/terminal.rs`, `src/pty/`, `src/sys/`, `src/pidtrack.rs`, `src/shell_env.rs`, `src/runtime_env.rs`, `src/shared/platform.rs`, `src/shared/terminal_presets.rs`.
- **Enforcement:** PTY/platform unit tests and native Windows/Linux gates.

### 7. Presentation, Configuration, and Update
- **Responsibility:** expose truthful CLI/TUI status, configuration, update behavior, bundled workflow guidance, documentation, and plugin metadata.
- **NOT responsibility:** infer delivery success or implement harness integration; Block 3 supplies relay health and Block 5 owns adapters.
- **Canonical modules:** `src/tui/`, `src/config.rs`, `src/update.rs`, `src/scripts.rs`, `src/commands/status.rs`, `src/commands/config.rs`, `src/commands/help.rs`, `src/commands/update.rs`, `src/commands/run.rs`, `README.md`, `skills/`, `plugin/`, `.claude-plugin/`, `gemini-extension.json`.
- **Enforcement:** CLI/TUI unit tests, update tests, and convention for documentation.

### 8. Validation and Release
- **Responsibility:** define reproducible Linux/Windows checks, real-tool/relay proofs, packaging, and release automation.
- **NOT responsibility:** compensate for incorrect production behavior with mocks; the owning production block must be corrected.
- **Canonical modules:** `tests/`, `scripts/`, `Justfile`, `.github/workflows/`, `Cargo.toml`, `Cargo.lock`, `dist-workspace.toml`, `install.sh`, `package.json`, `package-lock.json`, `pyproject.toml`, `tsconfig.json`, `.node-version`, `rust-toolchain.toml`, `LICENSE`.
- **Enforcement:** `just ci` and GitHub Actions.

## Autonomy Map

| Decision type | Autonomy | Examples |
|---|---|---|
| Implementation details inside an existing block | agent decides | private helpers, ACK tracking structure |
| Test additions using synthetic fixtures | agent decides | deterministic PUBACK and size-boundary tests |
| Ambiguous requirement or AC found mid-slice | ask first | two user-visible delivery meanings |
| New dependency, service, tool, or protocol version | ask first | MQTT library, daemon, fragmentation protocol |
| Persistence: new tables, migrations, schema changes | ask first | durable outbox schema |
| Security, auth, permissions, or relay trust model | ask first | disabling or expanding remote RPC |
| New block, boundary shift, or new source of truth | ask first | separate broker/worker/MCP |
| Public or external API contract changes | ask first | CLI JSON or MQTT envelope shape |
| Relay delivery and cursor semantics outside Rules 1–4 | ask first | weakening acknowledgement, changing the 128 KiB contract, or dropping events; implementing Rules 1–4 in the active repair is pre-decided |
| State mutation outside the workspace | ask first | push, release, broker reconfiguration, remote install |
| Deleting or weakening validation or tests | ask first | skipping Windows or real relay proof |
| Claiming unacknowledged relay delivery as success | **never** — reopen the product decision | enqueue treated as synchronized |
| Real customer data, credentials, or secrets anywhere | **never** | synthetic fixtures only |

## Validation

- `just ci fmt clippy test` — formatting, all-target lint, and unit/integration
  tests on the current host.
- `just ci` — the repository's complete local gate, including pinned real-tool
  and public-MQTT roundtrip tests.
- `cargo test --locked --test test_relay_roundtrip -- --ignored --nocapture --test-threads=1`
  — two isolated HCOM instances and remote RPC through a real MQTT broker.
- Windows: `just ci fmt clippy test` plus `just package-smoke-windows` — native
  compilation/tests and packaged-binary smoke.
- A repair touching relay delivery closes only after a private-broker
  Linux-to-Windows load scenario exceeds the former failing aggregate size,
  delivers a terminal marker, and records publisher cursor/status plus receiver
  evidence. A reset repair additionally closes only after native Windows reset
  archives/removes a populated WAL database and exits successfully.
- Unit tests and the single-host roundtrip do not prove Windows file locking,
  Bernardo's private broker, a long accumulated transcript, or peer handling;
  the physical proof above owns those claims. Model calls in the roundtrip use
  a local mock even though MQTT and HCOM processes are real.

## Execution Environment

- `rust-toolchain.toml` pins Rust 1.97.1 with Clippy and rustfmt; Cargo uses
  `Cargo.lock`.
- Tests must use disposable `HCOM_DIR` roots. Test guards reject unregistered
  production database paths.
- The ignored relay roundtrip uses an external MQTT broker and performs remote
  launch/kill inside temporary state; run it serially and allow its cleanup.
- Native Windows executables may be mapped by running agents. The Windows CI
  script parks a locked debug binary before rebuilding.
- Generated release workflow content is governed by `dist-workspace.toml` and
  `dist generate --check`; the current repair must not edit generated outputs.

## Extension Seams

- A new harness enters only through Block 5 and the shared Block 4 lifecycle.
- Relay transport changes enter only through Block 3 and must preserve the core
  invariant and encrypted-envelope compatibility.
- Database evolution enters only through Block 2 with an explicit schema
  decision and migration proof.
- An MCP, separate worker, broker service, scoped multi-tenant authorization, or
  new artifact transport is not built until separately requested and approved.

## Weak Spots (honest current state)

- `tests/test_relay_roundtrip.rs` proves a real broker path but not sustained
  oversized history or two physical operating systems. Relay changes may touch
  this test to add deterministic coverage and must add the physical proof named
  in Validation rather than treating the existing test as sufficient.
- The relay is deliberately one full-trust domain. New work must preserve and
  clearly document that boundary or stop for an explicit security redesign.
- Several runtime responsibilities converge in large router/launcher/terminal
  modules. This repair may make the minimum reset dispatch change in
  `src/router.rs`; unrelated decomposition routes around them.
- `src/relay/client.rs` currently drives pushes from the same event loop that
  receives PUBACK and discards both PUBACK and outgoing packet identifiers in
  normal handling. The repair may restructure this flow inside Block 3, but it
  must not block the event loop while waiting for the acknowledgement it owns.

## Open / Undecided

- None for the current relay-delivery and Windows-reset repair.

## Operational Facts

- Upstream is `aannoo/hcom`; Bernardo's fork is `bernardofortes/hcom`.
- No public pull request or default fork branch inspected on 2026-08-08 fixes
  the normal relay's missing PUBACK gate or the Windows reset handle ownership;
  this fork carries the bounded repair rather than importing an unknown fork.
- The 128 KiB ceiling is the compatibility contract with the deployed HCOM
  0.7.24 clients in this environment, not a broker-reported peer-discovery
  mechanism. Changing it or adding packet fragmentation requires a separate
  protocol decision.
- Local implementation and commits are authorized. Push, upstream PR, release,
  installation on another machine, and broker configuration remain separately
  authorized external mutations.
- The active operational broker is private and must not be replaced, exposed,
  or reconfigured as part of this repair.
