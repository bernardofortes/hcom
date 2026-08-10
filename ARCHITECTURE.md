# Architecture — HCOM Fork

**Status:** Current truth. Confirmed by Bernardo on 2026-08-10.
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

For this fork, stability also means that a producer receives an actionable
failure before an oversized event becomes durable, and that one HCOM
installation keeps the same relay device identity and assigned short name
across process restarts, binary reinstalls, and ordinary database resets while
its `HCOM_DIR` is preserved.

## The Core Invariant

A cross-device relay accepts a locally produced event only when its complete
serialized relay representation is within the canonical event-admission
budget. An oversized command fails synchronously and visibly before the
original event is persisted; an oversized RPC result becomes a small explicit
error result. Accepted events remain ordered and advance the durable relay
cursor only after the complete MQTT PUBLISH fits the compatibility ceiling and
the broker acknowledges that exact publication.

Disconnected, timed-out, state/envelope-oversized, or otherwise unconfirmed
publication remains retryable and is reported as error, never success. PUBACK
does not prove peer handling. The persisted relay UUID and assigned short name
are also stable installation identity while `HCOM_DIR` survives. These rules
come from two incidents: HCOM 0.7.24 reported `sent` and `connected` without
Windows-to-Linux delivery; an oversized transcript RPC entered the durable
FIFO, and a restored device could reappear under another generated name.

## Cross-Cutting Rules

1. MQTT enqueue, broker acknowledgement, peer receipt, and agent handling are
   distinct states; code and user-visible status must not collapse them.
2. `MAX_RELAY_PACKET_BYTES` remains the canonical 128 KiB complete-packet
   compatibility ceiling of the deployed HCOM 0.7.24 peers. Client inbound
   setup, outgoing construction, and tests use it. Event batching keeps the
   complete MQTT PUBLISH within it; no dynamic peer-limit discovery is
   introduced.
3. Block 3 exports `MAX_RELAY_EVENT_BYTES = 64 KiB` for the complete serialized
   event value, measured with a worst-case event ID. Block 2 enforces it before
   every local event insert. Only the explicit authenticated-peer import path
   is exempt; a JSON `_relay` field alone never bypasses admission. The raw
   transaction insert used by instance finalization routes through the same
   validator. BBF5 retires the 1 MiB `MAX_MESSAGE_SIZE` acceptance claim:
   producer preflight and user-facing errors must not promise more than the
   serialized event admission will accept.
4. Over-budget CLI operations return non-zero with actual and allowed sizes.
   Over-budget RPCs persist only a bounded error. The original receives no ID,
   row, or queued-success wording. The central gate logs rejection for internal
   producers; caller-controlled producers propagate it through their existing
   error channel.
5. Exact packet construction remains the last defense. If current state plus
   an admitted event still does not fit, publication fails and the cursor does
   not move; accepted data is never truncated, skipped, omitted, or invented.
6. QoS 1 cursor advancement remains tied to matching PUBACK, with a bounded
   timeout and explicit connection error. Only receiver evidence proves peer
   decoding.
7. Database reset stops owned processes and releases every in-process SQLite
   handle before archiving or deleting database and WAL/SHM files.
8. Relay UUID and assigned short name live at `HCOM_DIR/device_id` and
   `HCOM_DIR/device_name`. BBF5 atomically migrates the legacy UUID and the
   existing local `relay_uuid_short` mapping when present; otherwise it records
   the natural short name. Restart, binary reinstall, and ordinary reset
   preserve both. Only fresh install or confirmed reset-all regenerates them.
   Conflicting local identity sources are typed fatal errors: CLI exits
   non-zero, the worker does not start, and no empty or generated fallback is
   allowed. The local durable mapping is restored before peer processing. A
   remote UUID claiming that short name is rejected and logged without
   changing or stopping the local identity.
9. Full-trust relay security and private-broker preference remain unchanged.
   Transport and identity repair do not broaden remote control. Only fully
   trusted devices may enroll, preferably through a private broker.
10. BBF5 adds no dependency, service, schema, protocol version, fragmentation,
    artifact transport, renaming, lease/registry, MCP, or generic remote
    execution.

## Routing Table — Where Does It Go?

| You are adding or changing… | It belongs in… | Never in… |
|---|---|---|
| CLI parsing and command dispatch | `src/main.rs`, `src/router.rs`, command entrypoints in `src/commands/` | relay transport or harness hooks |
| SQLite schema, local event admission, and shared database operations | `src/db/`, `src/paths.rs`, `src/commands/reset_ops.rs` | TUI, MQTT client/event loop, or tool adapters |
| MQTT connection, packet/event budgets, relay cursors, push/pull, encryption, replay, and remote RPC | `src/relay/`, `src/commands/relay.rs` | CLI presentation or harness-specific hooks |
| Persistent relay UUID/short name, legacy migration, and reset semantics | `src/relay/mod.rs`, `src/paths.rs`, `src/commands/reset_ops.rs` | agent-name allocation, new transient `.tmp` identity state, or harness hooks |
| Agent identity, lifecycle, launch, and local delivery | `src/identity.rs`, `src/instance_*`, `src/launcher.rs`, `src/delivery*` | relay serialization or TUI rendering |
| Harness integration and transcripts | `src/hooks/`, `src/tools/`, `src/transcript/`, plugin sources | relay transport or database migrations |
| Terminal, PTY, and platform process behavior | `src/terminal.rs`, `src/pty/`, `src/sys/`, `src/pidtrack.rs` | relay state, TUI, or database ownership |
| Dashboard, configuration, update, and user guidance | `src/tui/`, `src/config.rs`, `src/update.rs`, `README.md`, `skills/`, plugin manifests | relay health derivation or test fixtures as authority |
| Cross-platform and real-boundary proof | `tests/`, `scripts/`, `Justfile`, `.github/workflows/`; inline unit tests stay beside private code under `src/` | production behavior |

## Required System Shape / Call Flow

1. A harness is launched or registered through `src/launcher.rs`,
   `src/commands/start.rs`, and harness-specific hooks.
2. Commands enter through `src/main.rs` and `src/router.rs`. Before `src/db/`
   persists a local event, Block 2 applies Block 3's canonical event budget.
   Authenticated pull uses a separate imported-event insertion path. Rejected
   producers return synchronously and no original row is written.
3. Local delivery in `src/delivery.rs` and `src/notify/` injects or wakes local
   agents. Terminal and TUI blocks expose the same stored state.
4. When relay is enabled, `src/relay/worker.rs` owns the relay subprocess and
   `src/relay/client.rs` owns the persistent MQTT connection/event loop.
5. `src/relay/push.rs` builds an encrypted retained state snapshot plus a
   packet-budgeted event batch. The client publishes it at QoS 1 and only a
   matching PUBACK permits the durable relay cursor and broker-confirmed status
   to advance.
6. `src/relay/pull.rs` authenticates, replay-checks, and applies remote state;
   `src/relay/control.rs` owns the existing full-trust remote RPC contract.
7. Reset dispatch coordinates process shutdown and transfers database file
   lifecycle to `src/commands/reset_ops.rs` only after the router-owned SQLite
   handle is released.
8. `src/relay/mod.rs` supplies one typed durable identity to every own-device
   reader: published state and RPC construction, suffix stripping, CLI,
   reset-event logging, TUI, tests, and worker. `device_short_id_for_db`
   remains only for remote identities. Reset-all is the sole reset path allowed
   to remove durable identity.

## Blocks

### 1. Application and Command Surface
- **Responsibility:** own shared application primitives, parse CLI intent, establish identity context, and route command entrypoints to one domain owner.
- **NOT responsibility:** implement relay delivery, database file lifecycle, terminal processes, or harness behavior; Blocks 3, 2, 6, and 5 own those.
- **Canonical modules:** `src/main.rs`, `src/router.rs`, `src/cli_context.rs`, `src/bootstrap.rs`, `src/core/`, `src/shared/` except the exact Block 6 files named below, `src/messages.rs`, `src/log.rs`, `src/integration_spec.rs`, and command entrypoints in `src/commands/` except the exact domain-owned files named below.
- **Enforcement:** CLI smoke/unit tests, producer-visible command error tests, and command exit codes.

### 2. Local State
- **Responsibility:** own SQLite schema, centralized local-event admission before persistence, events, instances, shared database access, bindings, and safe database file operations.
- **NOT responsibility:** choose MQTT/event ceilings, decide relay cursor or delivery semantics, or render state; Blocks 3 and 7 own those.
- **Canonical modules:** `src/db/`, `src/commands/reset_ops.rs`, `src/commands/archive.rs`, `src/paths.rs`.
- **Enforcement:** event-size boundary tests, database unit tests, and Windows reset runtime proof.

### 3. Cross-Device Relay
- **Responsibility:** own MQTT lifecycle, packet/event budget contracts, durable relay UUID/short name, bounded encrypted envelopes, acknowledged publication, pull/replay, health, and existing remote RPC transport.
- **NOT responsibility:** execute agent lifecycle operations, redefine their authority, or silently truncate, drop, or fabricate rejected producer data; Block 4 and the documented security model own authority, Block 2 enforces admission, and producer owners surface rejection.
- **Canonical modules:** `src/relay/`, `src/commands/relay.rs`.
- **Enforcement:** producer-visible oversize failure, identity migration, relay unit tests, real MQTT roundtrip, and cross-device load proof.

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
| Persistence: new tables, migrations, schema changes | ask first | durable outbox schema; the BBF5 identity-file migration under Rule 8 is pre-decided |
| Security, auth, permissions, or relay trust model | ask first | disabling or expanding remote RPC |
| New block, boundary shift, or new source of truth | ask first | separate broker/worker/MCP |
| Public or external API contract changes | ask first | CLI JSON or MQTT envelope shape |
| Relay delivery, admission, identity, and cursor semantics outside Rules 1–8 | ask first | weakening acknowledgement/admission, changing the 128 KiB/64 KiB contracts, deleting identity outside reset-all, dropping accepted events, or adding fragmentation; implementing Rules 1–8 in BBF5 is pre-decided |
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
- Event-admission work closes only when oversized CLI and remote-RPC input
  produces no original event ID/row, returns an actionable error, and leaves a
  following small event able to cross real MQTT and advance only on matching
  PUBACK. A complete event at the advertised boundary must pass, and
  JSON-escaping overhead must be measured rather than guessed.
- Identity work closes only when creation, legacy UUID/name migration,
  same-value coexistence, conflicting local sources, concurrent first use,
  remote short-name collision, restart, ordinary reset, and explicit reset-all
  are proved with disposable `HCOM_DIR` roots. Reset-event logging and TUI must
  expose the actual canonical value; published `short_id` and own-suffix
  stripping must use the durable name after ordinary reset. Native Windows
  GIGA must preserve its short name across install and daemon restart.
- Unit tests and the single-host roundtrip do not prove Windows file locking,
  Bernardo's private broker, a long accumulated transcript, or peer handling;
  the physical proof above owns those claims. They also do not prove an
  aggregate state-only packet fits, repair pre-BBF5 poisoned history, or recover
  identity after all of `HCOM_DIR` is lost. Model calls in the roundtrip use a
  local mock even though MQTT and HCOM processes are real.

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
  `dist generate --check`; BBF5 must not edit generated outputs.

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
  modules. The reset dispatch is intentionally minimal; unrelated decomposition
  routes around these modules.
- State snapshots aggregate all local instances and path/detail fields without
  total-size admission. BBF5's 64 KiB event budget reserves half the packet for
  ordinary state but, by owner ruling, does not redesign or cap extreme
  aggregate state. If state alone exceeds 128 KiB, exact packet defense fails
  without cursor movement; stronger handling requires a separate package.
- A pre-BBF5 database may already contain an oversized event. BBF5 does not
  rewrite accepted history; ship validation must prove both target queues are
  clean or stop for explicit reset/recovery approval.

## Open / Undecided

- None for BBF5; extreme aggregate state and pre-BBF5 recovery are bounded
  under Weak Spots.

## Operational Facts

- Upstream is `aannoo/hcom`; Bernardo's fork is `bernardofortes/hcom`.
- This fork carries BBF4 at `0.7.24+bbf.4` on
  `fix/relay-delivery-windows-reset`; BBF5 is the approved next stability
  package and does not claim upstream delivery.
- The 128 KiB ceiling is the compatibility contract with the deployed HCOM
  0.7.24 clients in this environment, not a broker-reported peer-discovery
  mechanism. Changing it or adding packet fragmentation requires a separate
  protocol decision.
- Local architecture, scope, and implementation commits are authorized. For
  BBF5, the owner separately authorized native install and relay-daemon restart
  on Windows GIGA for final proof and push of the reviewed result to
  `fix/relay-stability-bbf5` at `bernardofortes/hcom`. Queue reset/recovery,
  upstream PR, release publication, broker reconfiguration, and every other
  external mutation remain ask-first.
- The active operational broker is private and must not be replaced, exposed,
  or reconfigured as part of this repair.
