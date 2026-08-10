# Product Change Register

## Implementation History

| Date | ID | Implementation |
|---:|---|---|
| 2026-08-10 | BUG-0003 | Preserved each installation's relay identity and short name across supported restarts, reinstalls, and ordinary resets on Linux and Windows. |
| 2026-08-10 | BUG-0002 | Prevented oversized locally produced events and RPC results from entering and blocking the cross-device relay queue. |
| 2026-08-10 | BUG-0001 | Restored reliable cross-device relay publication and Windows database reset behavior in the maintained HCOM fork. |

## Active Items

| ID | Description | Added on | Priority |
|---|---|---:|---|
| TECH-0001 | Bound or redesign extreme aggregate relay state snapshots that can exceed the MQTT packet ceiling independently of events. | 2026-08-10 | Low |
