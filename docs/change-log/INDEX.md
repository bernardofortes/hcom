# Product Change Register

## Implementation History

| Date | ID | Implementation |
|---:|---|---|

## Active Items

| ID | Description | Added on | Priority |
|---|---|---:|---|
| BUG-0001 | Restore reliable cross-device relay publication and Windows database reset behavior in the maintained HCOM fork. | 2026-08-08 | High |
| BUG-0002 | Prevent oversized locally produced events from entering and blocking the cross-device relay queue. | 2026-08-10 | High |
| BUG-0003 | Keep one HCOM installation's relay identity and short name stable across supported restarts, reinstalls, and ordinary resets on Linux and Windows. | 2026-08-10 | Medium |
| TECH-0001 | Bound or redesign extreme aggregate relay state snapshots that can exceed the MQTT packet ceiling independently of events. | 2026-08-10 | Low |
