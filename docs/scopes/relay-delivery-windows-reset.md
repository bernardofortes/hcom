# Package: Reliable relay delivery and Windows reset

Status: approved (2026-08-08)
Lane: Fix
Run level: implement
Roles: writer: gpt (GPT-5.6 Sol) · effort xhigh · write; reviewer: claude-opus (Claude Opus) · effort high · read+execute, no product write · fresh

## Original Request (verbatim)

> E outra coisa: se a gente está falando de MCP, quer dizer que, por definição, a gente vai ter aberto a gente para poder ter acesso ao MCP. Então é melhor fazer vmcp ou consertar o SH com não sei Se a gente for fazer um MCP, eu não acho que vale a pena fazer um MCP em cima do hcom\. Se a gente quer consertar o INTERCOM e usar, a gente pode usar agora para implementação que eu preciso fazer. Então se for rápido consertar o HCOM, eu quero fazer isso de qualquer forma.

> Ok podemos fazer isso por aqui. Você faz isso, usa o BBF e faz essa correção dele até o final. Já está pré-autorizado para você fazer a correção dele do HCOM, ok?

> Me diga, faz um sumário do que é. Eu aprovo por aqui já, baseado no seu sumário

> ok

## Outcome and Bounds

- Outcome: a Linux HCOM and a Windows HCOM can exchange a relay backlog larger than the former failing aggregate without silently losing the terminal event, while the Windows installation can reset its populated database without failing on its own open SQLite files.
- Context/premises: source inspection confirms that normal relay push advances its cursor immediately after MQTT enqueue and does not consume the matching PUBACK; `set_max_packet_size(128 KiB)` constrains incoming packets rather than the sender's complete packet; reset deletes database sidecars while the router still owns an open `HcomDb`. The reported HCOM 0.7.24 incident counts — 764 publishes, 373 over 128 KiB, a maximum of 211,174 bytes, and a later unproven 14,281-byte push — remain unverified incident evidence until reproduced by the scoped proofs.
- Non-functional: preserve the existing MQTT envelope and full-trust relay compatibility; complete MQTT PUBLISH packets must not exceed 128 KiB; no new dependency, schema, service, protocol version, or security authority.
- Out of Scope: MCP or Intercom development; a new broker or worker; MQTT fragmentation or transcript/artifact transport; dynamic peer-limit discovery; end-to-end security redesign; remote launch changes; Mosquitto configuration; unrelated HCOM cleanup or refactoring; push, pull request, release, remote installation, or broker mutation.

## Requirements

- R1 — (changes existing) Bound every complete retained relay-state MQTT PUBLISH packet to `MAX_RELAY_PACKET_BYTES = 128 KiB`. Adaptively batch new events without truncation, omission, or protocol change; if one required state/event unit cannot fit, report an explicit error and leave its durable cursor unchanged.
- R2 — (changes existing) Allow at most one relay-state publication in flight, associate it with the MQTT packet identifier, and advance `relay_last_push`, `relay_last_push_id`, synchronization time, and broker-confirmed status only after the matching PUBACK. Timeout, disconnect, or wrong ACK remains retryable and visible as error. CLI/TUI wording distinguishes queued or broker-confirmed state from peer delivery.
- R3 — (changes existing) Make reset stop owned processes and release every in-process SQLite handle before archiving/removing the database, WAL, and SHM files, preserving the existing preview, hooks, and `--all` behavior.

## Contract Propagation

| Changed contract | Producer/authority | Consumers to update and prove |
|---|---|---|
| `MAX_RELAY_PACKET_BYTES` is the complete-packet compatibility ceiling | `src/relay/mod.rs`, packet construction in `src/relay/push.rs` | client inbound configuration in `src/relay/client.rs`; relay unit and real-MQTT tests |
| MQTT enqueue, PUBACK, broker confirmation, and peer delivery are distinct states | event loop and pending publication state in `src/relay/client.rs` and `src/relay/push.rs` | relay status in `src/commands/relay.rs`; send output in `src/commands/send.rs`; TUI output in `src/tui/actions.rs`; CLI smoke and roundtrip tests |
| Reset owns the database-file lifecycle only after all live handles are released | command dispatch in `src/router.rs`; reset coordination in `src/commands/reset.rs`; file operations in `src/commands/reset_ops.rs` | reset unit tests and native Windows reset proof |

## Contract and Phase E

| AC | Exact observable behavior | R ids | Slice | State | Running-app evidence |
|---|---|---|---:|---|---|
| AC-1 | When an isolated sender accumulates more than 211,174 bytes of individually fitting events, it emits only complete MQTT PUBLISH packets at or below 128 KiB and the receiving peer observes every new event including a terminal marker, without omission. | R1 | 1 | implemented | Local real-MQTT proof: 29 events over 211,174 aggregate text bytes reached the isolated receiver including the terminal marker; final conductor roundtrip passed in 122 s. Physical Linux-to-Windows/private-broker proof pending. |
| AC-2 | When a relay-state publication lacks its matching PUBACK, its cursor and sync status do not advance and the state remains retryable; after the matching PUBACK, exactly that publication commits and later batches drain serially. User-visible output says queued or broker-confirmed, never peer-delivered without receiver evidence. | R2 | 1 | implemented | Deterministic ACK/status/reconnect tests passed within 134/134 relay tests; CLI smoke passed 26/26; local real-MQTT receiver/cursor proof passed. Physical private-broker proof pending. |
| AC-3 | On native Windows with a populated WAL database, reset exits successfully, leaves the expected archive, and can replace/remove the database, WAL, and SHM without a file-lock error while preserving reset hooks and `--all`. | R3 | 1 | implemented | Linux isolated populated-WAL archive/removal and CLI reset passed; native Windows file-lock/package proof pending. |

## Architecture and Decisions

- Lives in: Block 1 Application and Command Surface; Block 2 Local State; Block 3 Cross-Device Relay; Block 7 Presentation, Configuration, and Update; Block 8 Validation and Release.
- Verdict: verified unchanged @b2a088b
- Open Decisions: none

## Slices

| # | Observable outcome + owned R/AC or `repairs AC-X` | Execution boundary · risk/split | Exact proof commands · owner | State / in-flight dispatch | Commit · review · corrected findings/gaps |
|---|---|---|---|---|---|
| 1 | Reliable bounded relay publication plus unlock-safe Windows reset · R1–R3 · AC-1–AC-3 | Exact hand-written paths listed below · high · 5 blocks/4 behavior clusters/3 ACs/11 paths; retained as one Fix because the approved HCOM failure repair and its shared close proof are one bounded release candidate; no generated outputs | `cargo test --locked relay::`; `cargo test --locked reset`; `cargo test --locked --test cli_smoke`; `PATH=/home/ubuntu/.cargo/bin:$PATH just ci fmt clippy test`; `PATH=/home/ubuntu/.cargo/bin:$PATH just ci mock-tools test_relay_roundtrip`; native Windows `just ci fmt clippy test`, `just package-smoke-windows`, and the isolated private-broker load/reset procedure defined by AC-1–AC-3 · writer/conductor | reviewed · boundary f7adb7fead3a40f22526dd887f09cfdf3f19968a · task/session e3345347-9ff7-4604-9ed7-d5ca2a390eb1 | Accepted upstream-code candidate: relay 134/134; reset 37/37 plus all 3 reset smokes under `--test-threads=1`; CLI smoke 26/26; `just ci fmt clippy test` 3/3 in 37 s; complete `just ci` 9 passed/1 skipped in 326 s after installing pinned `cargo-dist 0.32.0`; retained real-MQTT proof passed in 121 s; `cargo fmt --check` and `git diff --check` passed. Focused Opus re-review returned ACCEPT with no Medium/High finding. Fork-only BBF4 metadata was then verified through `hcom --version` and 7/7 focused reconnect/PUBACK tests. Windows/private proof pending. |

## Generated Boundary

| Slice | Generation route | Approved generated-output roots | Pre-write whole-worktree fingerprint | Pre-existing drift + owner disposition | Convergence evidence + actual path manifest |
|---:|---|---|---|---|---|
| 1 | no generated outputs | none | not applicable | none | not applicable |

Generated-only rule: outputs are produced only by the declared generation route; writer and conductor never manually edit them.

## Fix Execution Boundary

- Additional context: normal relay push currently publishes at QoS 1 and immediately persists success; normal event handling discards PUBACK/outgoing identifiers; the push function runs from the same event loop that must keep polling; reset currently receives a borrowed router-owned `HcomDb`. The writer must inspect the exact rumqttc v5 packet-size and event semantics rather than infer them.
- Allowed writes: `src/relay/mod.rs`, `src/relay/push.rs`, `src/relay/client.rs`, `src/commands/relay.rs`, `src/commands/send.rs`, `src/tui/actions.rs`, `src/router.rs`, `src/commands/reset.rs`, `src/commands/reset_ops.rs`, `tests/cli_smoke.rs`, `tests/test_relay_roundtrip.rs`.
- Package path: `docs/scopes/relay-delivery-windows-reset.md` is conductor-owned and forbidden to writer.
- Autonomy stop: on any FRAMEWORK/project-authority stop or unowned decision, stop unchanged and report. In particular, stop before adding a dependency, schema, service, protocol version, fragmentation, security/trust change, remote-control change, or write outside the allowed paths.
- Required tests: deterministic complete-packet boundary, single-unit oversize failure, ordered adaptive drain, missing/wrong/matching PUBACK state transitions, truthful CLI/TUI wording (AC-1/AC-2); native Windows populated-WAL reset with archive and hooks/`--all` coverage (AC-3); real private-MQTT cross-device terminal-marker delivery (AC-1/AC-2).
- Git rules: writer may not stage, commit, push, reset, clean, or edit the package/register/architecture. The boundary tree is clean at `b2a088b`; any unexpected path change is an immediate stop.
- Writer report: files changed · mechanism · every command/result · decisions · any stop.

## Ready

- [x] Open Decisions is none
- [x] every promise/Requirement has an observable AC
- [x] every AC is reachable and owned by exactly one slice
- [x] architecture verdict recorded; amendment committed when required
- [x] lane, run level, roles, bounds and exclusions recorded
- [x] every slice has boundary, risk/split evidence and proof commands; Fix has exact paths
- [x] rewrite retention inventory complete when applicable
- [x] changed-contract propagation complete when applicable
- [x] Generated Boundary approval fields are complete per slice; implementation evidence may be `pending`, and Full packets only copy approval fields
- [x] ship target, recovery and rollback compatibility planned when applicable
- [x] Full pressure findings disposed (Full only)

## Exceptions

- Fix round 1 — conductor validation finding, Medium: `src/relay/mod.rs` accepted `connected` and `queued` as live inputs but persisted both as `ok`, making raw status claim broker confirmation before the matching PUBACK and contradicting R2/AC-2. Repair must persist the exact live state and prove `connected` → `queued` → `ok` across enqueue and matching ACK.
- Evidence gap — the exact parallel command `cargo test --locked reset` passed in writer runs but failed once for the unchanged `config::tests::test_reset_allows_reinit`: another reset-filtered test initialized the process-global `Config` between its reset and environment setup. The isolated exact test passed and the complete reset filter passed 37/37 plus reset smoke with `--test-threads=1`; this out-of-scope pre-existing test-isolation race was not changed.
- skill lesson: `bbf-implement` configures the Claude Opus reviewer as read+execute, but the canonical `ask` Claude read rail disables Bash while its write rail is reserved for tasks that must change files. The review therefore uses the configured fresh Opus in read-only mode over the complete candidate and conductor-produced execution evidence; independent reviewer command execution is unavailable on this rail.
- Fix round 2 — fresh Opus review session `a7b6f5a9-e90d-45f4-84a1-922d6905ab87` returned ACCEPT with no Medium/High findings. Two Low requirement-preservation findings were accepted for repair: the reset ownership early-return defers pending-message delivery on non-destructive preview/`reset hooks` paths (`src/router.rs`), and Linux-testable `reset hooks`/`reset all` behavior lacks end-to-end CLI proof (`tests/cli_smoke.rs`). The other five Low observations do not contradict the selected requirements and remain outside this Fix.
- Final review — fresh Opus session `213d277c-8632-4e0d-9eaa-442e2adebe0b` returned ACCEPT with no Medium/High findings. Its Low failure-path observation is intentionally retained: if database archive/removal fails, `reset all` now stops before hooks/config/device cleanup rather than continuing a wider partial reset. This fail-fast, all-or-nothing boundary is the safe interpretation of R3 and the global failure rule; normal preview, hooks-only, reset, and reset-all behavior remains covered. Four other Low observations were non-blocking and outside this Fix.
- Fix round 3 — upstream-candidate Opus review session `e3345347-9ff7-4604-9ed7-d5ca2a390eb1` found one Medium R2 retryability defect: after rumqttc discarded unacknowledged packets on a new-session reconnect, HCOM retained its volatile pending marker, blocked every later push, and eventually exited on timeout. The correction releases only that volatile marker when `CONNACK.session_present` is false, immediately retries from the unchanged durable cursor, preserves resumed-session retransmission and matching-PUBACK gating, and adds deterministic new-session/resumed-session coverage. The same reviewer confirmed the bounded correction with `ACCEPT`; no Medium/High finding remains. Four Low observations remain non-blocking under Scope Lock: pre-CONNACK status noise, graceful-shutdown timing, local-send wording scope, and explicitly documenting native-CA broker-probe behavior in the upstream PR.
- Validation environment — the first complete `just ci` invocation stopped before candidate checks because pinned `cargo-dist` was absent (`dist-check`, exit 127). After installing the repository-declared `cargo-dist 0.32.0`, the one permitted rerun passed 9 steps with only the optional MSRV toolchain check skipped. Typecheck removed one unrelated `hasShrinkwrap` lockfile metadata line; that generated drift was excluded from the candidate.
