# Package: Relay Stability BBF5

Status: approved (2026-08-10)
Lane: Full
Run level: ship
Ship target (ship only): user-global HCOM on this Linux host and Windows GIGA through the existing private broker; reviewed branch `fix/relay-stability-bbf5` at `bernardofortes/hcom`; no public release
Roles: writer `gpt` (GPT-5.6 Sol) · effort xhigh · write; reviewer `claude-opus` (Claude Opus) · effort high · read+execute · fresh; pressure-test `kimi` (Kimi K3) · effort high · read · fresh

## Original Request (verbatim)

> Parece que tem um bug ainda no BBF4 que tem que ser consertado. Olha o que o outro agente falou :   Sim. O reset tratou o sintoma; existe uma causa raiz no produto.
>
> O que aconteceu:
>
> 1. Uma máquina pediu remotamente um transcript.
> 2. O HCOM gerou uma resposta RPC com aproximadamente 214,6 KB de conteúdo.
> 3. Essa resposta inteira foi gravada como um único evento rpc_result.
> 4. O relay aceita pacotes de no máximo 128 KB.
> 5. Como esse evento era indivisível, ele bloqueou a fila FIFO inteira.
>
> A BBF4 já quebra lotes de vários eventos pequenos em vários pacotes. O que ela não faz é fragmentar um único evento. Isso foi deliberadamente excluído do escopo porque exigiria protocolo de fragmentação, numeração, remontagem e compatibilidade entre versões. O MQTT pode dividir os bytes no transporte TCP, mas o broker ainda enxerga um único PUBLISH de 215 KB.
>
> A falha real é um contrato inconsistente:
>
> - O RPC de events já limita sua resposta a aproximadamente 98 KB.
> - O RPC de transcript não possui limite equivalente.
> - O emissor central de rpc_result também aceita qualquer tamanho.
> - Um resultado grande demais entra na mesma fila ordenada e impede tudo que vem depois.
>
> Isso aparece diretamente em /C:/src/hcom-windows-validation-369ccee-clean/src/relay/control.rs:978 e no empacotamento em /C:/src/hcom-windows-validation-369ccee-clean/src/relay/
> push.rs:227.
>
> Minha recomendação é corrigir, sem criar fragmentação MQTT genérica:
>
> - Estabelecer um limite central para qualquer rpc_result.
> - Fazer transcript usar paginação/faixas já suportadas.
> - Se ainda exceder o orçamento, retornar um erro pequeno e claro — nunca gravar o evento gigante.
> - Provar que uma resposta excessiva não bloqueia o próximo evento da fila.
>
> Assim eliminamos a classe inteira de “um RPC grande paralisa o relay”. O reset resolveu a ocorrência atual, mas sem essa correção outro transcript grande pode reproduzir o problema.

> Não, isso aconteceu na máquina, só pra você saber. Esse erro aconteceu na outra máquina mas eu quero que seja consertado aqui. Agora, esse problema mais amplo que você falou, que aceita até 1 MB, e aí não vai quebrar? Não vai deixar o risco de quebrar? Eu quero estabilidade. É isso que eu busco. E essa sua solução sugere isso?

> Isso mas a mensagem falhar de maneira loud significa que a LLM que tentou enviar essa mensagem sabe da limitação e pode fazer alguma coisa: fragmentar, fazer o que quiser. Aí você tá pro ponto certo?

> É isto que você está propondo correto?

> Perfeito! Eu gosto dessa abordagem!
>
> Agora um outro problema que eu percebi do westcon é que volta e meia ele caía e eu restaurava uma máquina e ela entrava com um nome diferente e se perdia. Daqui a pouco duas máquinas dentro do westcon apareciam três com nomes diferentes e não conseguia determinar um nome.
>
> O que eu gostaria que fosse esse modelo seria o seguinte: "Olha! Eu vou subir westcon. O nome da máquina é X. E acabou! Esse é o nome da máquina mas deu algum bug? Ao cair rodava de novo e ele entrava com outro nome?"
>
> Aí depois ele dizia: "Não você já tá com um nome anterior? Tem alguma instabilidade associada a essa parte de nome e identificação que eu queria que se fizesse uma investigação por favor?"

> Para simplificar eu só quero ter previsibilidade. Se ele entrou com um nome, que ele tenha sempre aquele nome, entendeu? Se eu sair e entrar de novo, facilita. Não quero mudar o programa. Pode ficar mais complexo. Quero o mínimo possível para rodar de maneira estável

> É simples fazer essa mudança?  Eu não quero gastar muito esforço com isso E tem que sobreviver, funcionar, por exemplo, em outra plataforma, Windows, não?

> Ok tá bom. Vamos fazer esses dois ajustes então. Você consegue criar, sei lá, não sei se são dois scopes para você executar em sequência ou se são slices para você integrar tudo? Como é que a gente poderia resolver isso tudo de uma maneira orquestrada para você, para fim a fim? E isto virá a próxima versão do IBF

> Ok tá bom. Vamos fazer esses dois ajustes então. Você consegue criar, sei lá, não sei se são dois scopes para você executar em sequência ou se são slices para você integrar tudo? Como é que a gente poderia resolver isso tudo de uma maneira orquestrada para você, para fim a fim? E isto virá a próxima versão do BBF

Owner direction rulings recorded during shaping:

> Seguir com BBF5 (Recommended)

> Validar na GIGA (Recommended)

> Nova branch BBF5 (Recommended)

> Dois ajustes mínimos (Recommended)

> Aprovar emenda (Recommended)

> Aprovar ambas (Recommended)

> ok

> Aprovar remoção (Recommended)

## Outcome and Bounds

- Outcome: an LLM or operator that attempts to create an oversized local event receives an immediate actionable failure before the event enters SQLite, a later small event still crosses the relay, and each preserved HCOM installation keeps its existing relay UUID and short name on Linux and Windows. The integrated artifact reports `0.7.24+bbf.5`.
- Context/premises: BBF4 already performs exact complete-packet measurement and PUBACK-gated cursor advancement. The reported 214.6 KB Windows event was not independently recovered on Linux; current source and deterministic tests confirm that an indivisible oversized event can block the FIFO. Current Linux identity is `RONI`, Windows is `GIGA`, and both derive from UUIDs under legacy `HCOM_DIR/.tmp/device_id`.
- Non-functional: retain the 128 KiB complete MQTT packet ceiling; enforce a 64 KiB complete serialized local-event ceiling; keep the paginated `events` RPC below that event ceiling after JSON serialization; no accepted local event may be silently dropped or rewritten; no identity fallback after a typed read/migration conflict; preserve current UUID and short name; support native Linux and Windows paths/locking; add no dependency, service, database schema, protocol version, or generated output.
- Out of Scope: MQTT/application fragmentation; transparent transport of arbitrary single units; artifact transport; device rename, lease, registry, or recovery after all of `HCOM_DIR` is lost; redesign/cap of extreme aggregate state snapshots; automatic rewrite/removal of pre-BBF5 oversized history; broker reconfiguration; upstream PR; public release.

## Requirements

- R1 — (changes existing) Block 3 exports `MAX_RELAY_EVENT_BYTES = 64 KiB`; Block 2 applies one exact serialized-event admission check before every local event insert, including instance-finalization's transactional insert, while authenticated peer imports use a separate explicit insertion path.
- R2 — (changes existing) An oversized caller-controlled CLI operation exits non-zero and reports actual and allowed serialized sizes; an oversized remote RPC persists only a bounded actionable error. The `events` RPC reduces its page budget below the 64 KiB complete-event ceiling and continues to truncate-and-deliver rather than crossing admission and becoming an error. The original oversized event receives no ID/row and is never reported queued.
- R3 — (already there, retained) Exact complete-packet measurement and matching-PUBACK cursor advancement remain the last defense. A rejected original cannot block a later small event, and accepted events are never skipped, truncated, or invented.
- R4 — (changes existing) The existing relay UUID and assigned short name migrate atomically to `HCOM_DIR/device_id` and `HCOM_DIR/device_name` under cross-platform locking. Same-value legacy/durable coexistence is accepted; conflicting local identity sources return a typed fatal error without fallback.
- R5 — (changes existing) Every own-device identity consumer uses the canonical durable result. Ordinary restart, binary reinstall, and database reset preserve UUID/name; reset-all alone removes them. The local mapping is restored before peer processing, and a remote UUID claiming the same short name is rejected/logged without renaming or stopping the local identity.
- R6 — (new) The coordinated release reports `0.7.24+bbf.5`, is installed and runtime-validated on this Linux host and Windows GIGA, preserves rollback to BBF4, and is pushed only to the authorized `fix/relay-stability-bbf5` branch.

## Retention Inventory

| Existing obligation | Survives where |
|---|---|
| 128 KiB complete MQTT PUBLISH ceiling | `MAX_RELAY_PACKET_BYTES`, exact packet construction, and packet-boundary tests |
| FIFO ordering and no omission of accepted events | packet builder, unchanged durable cursor, and matching-PUBACK gate |
| Existing 1 MiB raw-message validation | replaced by truthful pre-persistence serialized-event admission and actionable actual/allowed error; no 1 MiB acceptance claim remains |
| Authenticated remote event import | explicit peer-import persistence path exempt from local admission because the received packet already bounded it |
| Current legacy UUID | copied atomically to `HCOM_DIR/device_id`; matching legacy value retained through ship for BBF4 rollback |
| Current DB-assigned short name | copied to `HCOM_DIR/device_name` and restored to DB before peer processing |
| `reset all` factory-reset semantics | removes legacy and durable identity only after explicit preview/confirmation |
| Existing BBF4 PUBACK/new-session repair | unchanged relay client behavior and regression tests |

## Contract Propagation

| Changed contract | Producer/authority | Consumers to update and prove |
|---|---|---|
| 64 KiB complete serialized local-event admission | Block 3 relay budget authority | Block 2 local/transactional insertion; `src/shared/constants.rs` and message validation/send; bundles; central RPC result emission; `handle_remote_events`/`REMOTE_EVENTS_BYTE_CAP`; subscriptions/lifecycle error channels; boundary tests |
| Explicit local versus authenticated-peer insertion | Block 2 event persistence API | `src/relay/pull.rs`; every local `log_event`/`log_event_with_ts` caller; instance finalization transaction |
| Durable UUID and assigned short name | Block 3 typed identity authority | relay push/control/pull/worker; relay CLI; reset-event logging; reset-all cleanup; TUI; relay roundtrip/tests |
| `0.7.24+bbf.5` artifact identity | Block 8 release metadata | `Cargo.toml`, `Cargo.lock`, `pyproject.toml`, installed Linux/Windows binaries, `hcom --version` proof |

## Contract and Phase E

| AC | Exact observable behavior | R ids | Slice | State | Running-app evidence |
|---|---|---|---:|---|---|
| AC-1 | A local event at the 64 KiB serialized boundary is accepted, one byte over returns a typed error with no ID/row, an authenticated imported event is not re-admitted, and instance finalization cannot bypass the same gate. | R1 | 1 | implemented | Conductor reproduced boundary/no-row, authenticated import, transactional pre-delete rejection, and pre-BBF5 poison last-defense tests; all 4 passed. |
| AC-2 | An oversized `hcom send` exits non-zero with actual/allowed sizes and no queued-success text; an oversized transcript or other RPC produces only a small actionable error result, never the oversized original; `events` truncates within the new serialized budget and still delivers its page. | R2 | 2 | implemented | Real CLI process exited non-zero with actual/allowed sizes and no row/queued text; central RPC fallback and escaping-heavy successful `events` page passed focused and cumulative tests. |
| AC-3 | After an oversize rejection, a following small marker crosses a real MQTT relay and advances the sender cursor only after matching PUBACK; the receiver supplies separate evidence. | R2, R3 | 5 | planned | pending |
| AC-4 | Legacy UUID/name migrate without changing either value; concurrent first use agrees; restart and ordinary reset preserve both durable files. | R4, R5 | 3 | planned | pending |
| AC-5 | Conflicting local identity sources fail CLI/worker without fallback; reset-all alone clears both durable files; a remote short-name collision does not rename or stop the local identity. | R4, R5 | 3 | planned | pending |
| AC-6 | Published state/RPC, suffix stripping, relay status, reset event, TUI, worker, and tests all expose/use the same durable UUID/name after ordinary reset. | R5 | 4 | planned | pending |
| AC-7 | Linux and Windows GIGA report `hcom 0.7.24+bbf.5`; GIGA keeps its pre-install short name across daemon restart; both queues are preflight-clean; one marker travels Linux→GIGA and another GIGA→Linux with receiver evidence; relay health is connected; and the reviewed commit is present on the authorized GitHub branch. | R6 | 5 | planned | pending |

## Architecture and Decisions

- Lives in: Block 1 Application and Command Surface; Block 2 Local State; Block 3 Cross-Device Relay; Block 7 Presentation, Configuration, and Update; Block 8 Validation and Release.
- Verdict: amended doc-first @`27832d7`
- Open Decisions: none. On 2026-08-10 the owner waived Slice 2's three-block split trigger because separating its CLI proof would make the slice non-vertical, and approved reducing the `events` RPC page budget below the 64 KiB serialized-event ceiling.

## Slices

| # | Observable outcome + owned R/AC or `repairs AC-X` | Execution boundary · risk/split | Exact proof commands · owner | State / in-flight dispatch | Commit · review · corrected findings/gaps |
|---|---|---|---|---|---|
| 1 | One persistence boundary admits bounded local events, exempts only explicit authenticated imports, and covers transactional finalization · R1 · AC-1 | Blocks 2/3; admission authority + local/import APIs + transactional validator; roots `src/db/`, `src/relay/`, `src/instance_lifecycle.rs` · normal · 2 blocks/3 clusters/1 AC/3 canonical-module paths; no split trigger | `cargo test --locked relay_event_admission -- --nocapture`; `cargo test --locked imported_event_admission -- --nocapture`; `cargo test --locked finalize_stop_event_admission -- --nocapture` from repository root → exact boundary/no-row/import/transaction proof · writer then conductor | closed · boundary `35f25ca` · writer task `ses_0134031afffeDl4Y3Aj3vmnwYF` · reviewer task `ses_013390d64ffeuB82fk6ZC1pcpr` | commit `4fe49ef` · ACCEPT · conductor proofs 4/4; `just ci` 9 passed/1 skipped in 305 s · Low: Slice 2 must close temporary RPC silence; test name overstates rollback but behavior is correct |
| 2 | Caller-controlled message and RPC producers receive truthful actionable oversize failures while `events` still delivers a bounded page · R2 · AC-2 | Blocks 1/3 plus changed existing integration proof under Block 8; roots `src/shared/constants.rs`, `src/shared/mod.rs`, `src/messages.rs`, `src/commands/`, `src/core/`, `src/relay/`, `tests/cli_smoke.rs` · high · 3 blocks/3 clusters/1 AC/7 canonical-module paths; owner waived the block-count split trigger on 2026-08-10 because separating the CLI proof would make the slice non-vertical | `cargo test --locked oversized_message -- --nocapture`; `cargo test --locked oversized_rpc_result -- --nocapture`; `cargo test --locked remote_events_with_event_budget -- --nocapture`; `cargo test --locked --test cli_smoke oversized_send -- --exact --nocapture` from repository root → CLI exit/text/no-row, bounded central RPC error, and truncated-delivered `events` page · writer then conductor | reviewed fix round 1 · boundary `87f30b8` · writer tasks `ses_0132688fdffegREC35yuQ4sNc4`, `ses_0130f300affe6TRRogJVtMcf9N` · reviewer tasks `ses_0131cec90ffeLgf5ua0kjPcXA1`, `ses_0130b4fccffetKI2bQE5yc0zRO` · conductor candidate validation 7/7 passed | ACCEPT · prior Medium closed; accepted Low observations unchanged |
| 3 | Legacy identity migrates atomically and reset/conflict/collision boundaries preserve the chosen local identity · R4, R5 · AC-4, AC-5 | Blocks 2/3; durable authority + migration/locking + reset semantics + collision initialization; roots `src/paths.rs`, `src/commands/reset_ops.rs`, `src/relay/` · normal · 2 blocks/4 clusters/2 ACs/3 canonical-module paths; no split trigger | `cargo test --locked durable_device_identity -- --nocapture`; `cargo test --locked durable_device_identity_conflict -- --nocapture`; `cargo test --locked durable_identity_reset -- --nocapture`; `cargo test --locked remote_short_name_collision -- --nocapture` from repository root → migration/concurrency/conflict/reset/collision proof · writer then conductor | planned · baseline @`27832d7`: `cargo test --locked` 2,222 passed/0 failed/16 ignored | pending |
| 4 | Every own-device reader uses the canonical durable identity · R5 · AC-6 | Blocks 3/7; relay runtime propagation + reset/TUI propagation; roots `src/relay/`, `src/commands/relay.rs`, `src/db/mod.rs`, `src/tui/db.rs` · normal · 2 blocks/2 clusters/1 AC/4 canonical-module paths; no split trigger | `cargo test --locked durable_identity_consumers -- --nocapture`; `cargo test --locked durable_identity_after_reset -- --nocapture` from repository root → actual canonical value across state/RPC/suffix/CLI/reset/TUI/worker · writer then conductor | planned · baseline @`27832d7`: `cargo test --locked` 2,222 passed/0 failed/16 ignored | pending |
| 5 | One BBF5 artifact passes integrated real-relay and native Windows ship proof · R3, R6 · AC-3, AC-7 | Block 8; real relay regression + version metadata + Linux/Windows install/rollback evidence + authorized branch push; roots `tests/`, `Cargo.toml`, `Cargo.lock`, `pyproject.toml` · normal · 1 block/4 clusters/2 ACs/4 canonical-module paths; no split trigger | `cargo test --locked --test test_relay_roundtrip oversized_rejection_does_not_block_following_event -- --exact --ignored --nocapture --test-threads=1`; `cargo run --locked -- --version` from repository root → real MQTT follow-up and BBF5 version; native GIGA commands resolved in the slice packet from architecture's Windows procedure · writer then conductor | planned · baseline @`27832d7`: `cargo test --locked` 2,222 passed/0 failed/16 ignored; `cargo run --locked -- --version` = `0.7.24+bbf.4` | pending |

## Generated Boundary

| Slice | Generation route | Approved generated-output roots | Pre-write whole-worktree fingerprint | Pre-existing drift + owner disposition | Convergence evidence + actual path manifest |
|---:|---|---|---|---|---|
| 1 | no generated outputs | none | not applicable | none | not applicable |
| 2 | no generated outputs | none | not applicable | none | not applicable |
| 3 | no generated outputs | none | not applicable | none | not applicable |
| 4 | no generated outputs | none | not applicable | none | not applicable |
| 5 | no generated outputs | none | not applicable | none | not applicable |

Generated-only rule: outputs are produced only by the declared generation route; writer and conductor never manually edit them.

## Release

- Target and procedure authority: current Linux user-global binary at `/home/ubuntu/.local/bin/hcom`; Windows GIGA user-global binary through the existing HCOM control surface; private broker unchanged; GitHub branch per `ARCHITECTURE.md` Operational Facts.
- Recovery plan: before either install, record active version/path/hash and copy the working BBF4 binary to an explicit timestamped recovery path on that host; retain matching legacy identity files; rollback stops only the relay daemon, restores the recorded binary, restarts it, and verifies version/name/connected health. Before push, record `origin/fix/relay-delivery-windows-reset` at `1e2546e`; no force operation is permitted.
- Verified recovery point: pending
- Rollback compatibility: BBF5 keeps the matching legacy `.tmp/device_id` and database mapping through ship, introduces no schema/protocol change, and does not production-reset either target; BBF4 can therefore read the pre-existing identity and database after binary restoration. Exact pre-install readback remains pending.
- Deployed revision: pending
- Health and documented production-safe smoke: pending
- Rollback outcome: not required

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

- Owner ruling: BBF5 implements only event poison prevention and durable identity. Extreme aggregate state handling is deferred to `TECH-0001`.
- Architecture review: two fresh code-verified passes found nine Medium wording/authority defects. The confirmed amendment fixes canonical budget ownership, local/import separation, full-trust retention, typed identity failure, durable short-name propagation, identity migration autonomy, all own-device readers, remote collision behavior, and retirement of the misleading 1 MiB message acceptance claim.
- Evidence gap: the exact 214.6 KB event was observed on Windows and reported by the owner/other agent but was not present in the archived Linux database; the current code path and deterministic oversized-unit test independently establish the failure mechanism.
- Pressure test — F1 disposed by owner waiver: Slice 2 remains high-risk and vertical across Blocks 1/3/8 rather than splitting its CLI proof from behavior.
- Pressure test — F2 corrected: `handle_remote_events` and `REMOTE_EVENTS_BYTE_CAP` are explicit Slice 2 consumers; R2/AC-2 require a page that truncates and delivers within the 64 KiB serialized-event ceiling.
- Pressure test — F3 corrected: AC-7 requires physical marker delivery in both Linux→GIGA and GIGA→Linux directions after native install/restart; connected status alone is insufficient.
- Pressure test — F4 packet ruling: the only legacy identity source is `.tmp/device_id`; the durable sources are `device_id` and `device_name`. Dead `.tmp/device_uuid` and KV `device_uuid` readers are replaced with the canonical typed authority and must assert the actual value in Slice 4.
- Baseline environment correction: the first clean worktree was under `/tmp`, which intentionally made three production-path guard tests treat `CARGO_MANIFEST_DIR` as temporary; that invalid-location run produced 2,181 pass/3 guard failures. The identical clean commit `27832d7` rerun under repository-local ignored `.run/` produced 2,222 pass/0 fail/16 ignored and reported base version `0.7.24+bbf.4`.
- Owner amendment on 2026-08-10: Slice 2 adds `src/shared/constants.rs` solely to remove the contradicted 1 MiB raw-message acceptance constant; the approved 64 KiB serialized-event behavior, ACs, and all other boundaries are unchanged.
- Owner amendment on 2026-08-10 after Slice 2 review: add `src/shared/mod.rs` solely to remove the zero-consumer `MAX_MESSAGE_SIZE` reexport and close the review's only Medium; no behavior or other boundary changes.
