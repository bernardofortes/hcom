//! Push loop — build state snapshot and events, publish via MQTT.
//!
//! Batches up to 100 events per publish within the complete MQTT packet ceiling.
//! Tracks the broker-confirmed cursor via KV key `relay_last_push_id`.

use rumqttc::v5::mqttbytes::QoS;
use rumqttc::v5::mqttbytes::v5::Publish;
use serde::Serialize;
use serde_json::{Value, json};

use crate::db::HcomDb;
use crate::log;

use super::crypto;
use super::{
    MAX_RELAY_PACKET_BYTES, device_short_id_for_db, safe_kv_get, safe_kv_set, set_relay_status,
    state_topic,
};

const RETAINED_EVENT_TAIL: i64 = 50;
const MAX_NEW_EVENTS_PER_PACKET: usize = 100;

#[derive(Debug)]
struct EventRow {
    id: i64,
    value: Value,
}

/// A complete relay-state publication ready to enqueue. The cursor metadata is
/// intentionally carried beside the bytes and is committed only after the
/// event loop observes the matching successful PUBACK.
#[derive(Debug)]
pub(crate) struct PreparedPush {
    pub topic: String,
    pub sealed: Vec<u8>,
    pub packet_bytes: usize,
    pub event_count: usize,
    pub max_event_id: i64,
    pub has_more: bool,
}

#[derive(Serialize)]
struct PushPayload<'a> {
    state: &'a Value,
    events: &'a [Value],
}

/// Build current instance state snapshot for publishing.
/// Only includes local instances (no origin_device_id).
pub fn build_state(db: &HcomDb, device_uuid: &str) -> Value {
    let short_id = device_short_id_for_db(db, device_uuid);

    let instances = match db.conn().prepare(
        "SELECT name, status, status_context, status_detail, status_time, parent_name,
                directory, transcript_path,
                wait_timeout, last_stop, tcp_mode, tag, tool, background
         FROM instances WHERE COALESCE(origin_device_id, '') = ''",
    ) {
        Ok(mut stmt) => {
            let rows: Vec<_> = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,          // name
                        row.get::<_, Option<String>>(1)?,  // status
                        row.get::<_, Option<String>>(2)?,  // status_context
                        row.get::<_, Option<String>>(3)?,  // status_detail
                        row.get::<_, Option<f64>>(4)?,     // status_time
                        row.get::<_, Option<String>>(5)?,  // parent_name
                        row.get::<_, Option<String>>(6)?,  // directory
                        row.get::<_, Option<String>>(7)?,  // transcript_path
                        row.get::<_, Option<i64>>(8)?,     // wait_timeout
                        row.get::<_, Option<f64>>(9)?,     // last_stop
                        row.get::<_, Option<bool>>(10)?,   // tcp_mode
                        row.get::<_, Option<String>>(11)?, // tag
                        row.get::<_, Option<String>>(12)?, // tool
                        row.get::<_, Option<bool>>(13)?,   // background
                    ))
                })
                .ok()
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default();

            let mut map = serde_json::Map::new();
            for row in rows {
                let name = &row.0;
                // Skip internal instances
                if name.starts_with('_') || name.starts_with("sys_") {
                    continue;
                }
                map.insert(
                    name.clone(),
                    json!({
                        "enabled": true,
                        "status": row.1.as_deref().unwrap_or("unknown"),
                        "context": row.2.as_deref().unwrap_or(""),
                        "status_time": row.4.unwrap_or(0.0),
                        "parent": row.5,
                        "directory": row.6,
                        "transcript": row.7,
                        "wait_timeout": row.8.unwrap_or(86400),
                        "last_stop": row.9.unwrap_or(0.0),
                        "tcp_mode": row.10.unwrap_or(false),
                        "tag": row.11,
                        "tool": row.12.as_deref().unwrap_or("claude"),
                        "background": row.13.unwrap_or(false),
                        "detail": row.3.as_deref().unwrap_or(""),
                    }),
                );
            }
            Value::Object(map)
        }
        Err(_) => json!({}),
    };

    // Get reset timestamp (local only — exclude imported events)
    let reset_ts = db
        .conn()
        .query_row(
            "SELECT timestamp FROM events
             WHERE type = 'life' AND instance = '_device'
             AND json_extract(data, '$.action') = 'reset'
             AND json_extract(data, '$._relay') IS NULL
             ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten()
        .and_then(|ts| parse_iso_timestamp_to_epoch(&ts))
        .unwrap_or(0.0);
    let capabilities = json!(super::control::advertised_remote_capabilities());

    json!({
        "instances": instances,
        "short_id": short_id,
        "reset_ts": reset_ts,
        "capabilities": capabilities,
    })
}

fn load_event_rows(
    db: &HcomDb,
    comparison: &str,
    cursor: i64,
    order: &str,
    limit: usize,
) -> Result<Vec<EventRow>, String> {
    let sql = format!(
        "SELECT id, timestamp, type, instance, data FROM events
         WHERE id {comparison} ?1 AND instance NOT LIKE '%:%'
         AND instance != '_device'
         AND json_extract(data, '$._relay') IS NULL
         ORDER BY id {order} LIMIT ?2"
    );
    let mut stmt = db
        .conn()
        .prepare(&sql)
        .map_err(|e| format!("event query: {e}"))?;
    let rows = stmt
        .query_map(rusqlite::params![cursor, limit as i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| format!("event query: {e}"))?;

    rows.map(|row| {
        let (id, ts, event_type, instance, data_str) =
            row.map_err(|e| format!("event row: {e}"))?;
        let data: Value = serde_json::from_str(&data_str)
            .map_err(|e| format!("event {id} contains invalid JSON: {e}"))?;
        Ok(EventRow {
            id,
            value: json!({
                "id": id,
                "ts": ts,
                "type": event_type,
                "instance": instance,
                "data": data,
            }),
        })
    })
    .collect()
}

fn serialize_payload(state: &Value, events: &[Value]) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&PushPayload { state, events }).map_err(|e| format!("json: {e}"))
}

fn complete_publish_packet_size(topic: &str, sealed_payload_len: usize) -> usize {
    let mut publish = Publish::new(
        topic,
        QoS::AtLeastOnce,
        vec![0_u8; sealed_payload_len],
        None,
    );
    publish.retain = true;
    // QoS 1 always carries a two-byte packet identifier. A non-zero stand-in
    // makes Publish::size account for the same field rumqttc adds at enqueue.
    publish.pkid = 1;
    publish.size()
}

fn candidate_packet_size(topic: &str, state: &Value, events: &[Value]) -> Result<usize, String> {
    let plaintext_len = serialize_payload(state, events)?.len();
    let sealed_len = plaintext_len + crypto::HEADER_LEN + crypto::TAG_LEN;
    Ok(complete_publish_packet_size(topic, sealed_len))
}

pub(crate) fn ensure_complete_publish_packet_size(
    topic: &str,
    sealed_payload: &[u8],
) -> Result<usize, String> {
    let packet_bytes = complete_publish_packet_size(topic, sealed_payload.len());
    if packet_bytes > MAX_RELAY_PACKET_BYTES {
        return Err(format!(
            "relay state PUBLISH packet is {packet_bytes} bytes; compatibility limit is {MAX_RELAY_PACKET_BYTES} bytes"
        ));
    }
    Ok(packet_bytes)
}

/// Build one complete, packet-budgeted state publication. New events are always
/// selected in ID order and never skipped; once the backlog is empty, a later
/// retained snapshot includes as much confirmed tail context as fits. If the
/// first new event cannot fit with the state snapshot, preparation fails loudly
/// and leaves the durable cursor untouched.
pub(crate) fn prepare_push(
    db: &HcomDb,
    relay_id: &str,
    device_uuid: &str,
    psk: &[u8; 32],
) -> Result<PreparedPush, String> {
    let state = build_state(db, device_uuid);
    let topic = state_topic(relay_id, device_uuid);
    let last_push_id: i64 = safe_kv_get(db, "relay_last_push_id")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let mut retained =
        load_event_rows(db, "<=", last_push_id, "DESC", RETAINED_EVENT_TAIL as usize)?;
    retained.reverse();
    let new_rows = load_event_rows(db, ">", last_push_id, "ASC", MAX_NEW_EVENTS_PER_PACKET + 1)?;
    let new_limit = new_rows.len().min(MAX_NEW_EVENTS_PER_PACKET);

    let state_only_size = candidate_packet_size(&topic, &state, &[])?;
    if state_only_size > MAX_RELAY_PACKET_BYTES {
        return Err(format!(
            "relay state snapshot requires a {state_only_size}-byte MQTT PUBLISH packet; compatibility limit is {MAX_RELAY_PACKET_BYTES} bytes"
        ));
    }

    // During backlog drain, spend the packet budget only on ordered,
    // unconfirmed events. Repacking the already-confirmed tail beside every
    // new unit turns a two-packet backlog into a burst of near-limit packets.
    // Once no new events remain, the periodic retained snapshot rebuilds as
    // much of the confirmed tail as fits for late subscribers.
    let mut events: Vec<Value> = if new_rows.is_empty() {
        retained.iter().map(|row| row.value.clone()).collect()
    } else {
        Vec::new()
    };

    if let Some(first_new) = new_rows.first() {
        let packet_bytes =
            candidate_packet_size(&topic, &state, std::slice::from_ref(&first_new.value))?;
        if packet_bytes > MAX_RELAY_PACKET_BYTES {
            return Err(format!(
                "relay event {} requires a {packet_bytes}-byte MQTT PUBLISH packet with the current state snapshot; compatibility limit is {MAX_RELAY_PACKET_BYTES} bytes; cursor remains {last_push_id}",
                first_new.id
            ));
        }
    } else {
        while candidate_packet_size(&topic, &state, &events)? > MAX_RELAY_PACKET_BYTES {
            events.remove(0);
        }
    }

    let mut selected_new = 0;
    for row in new_rows.iter().take(new_limit) {
        events.push(row.value.clone());
        if candidate_packet_size(&topic, &state, &events)? > MAX_RELAY_PACKET_BYTES {
            events.pop();
            break;
        }
        selected_new += 1;
    }

    let max_event_id = if selected_new == 0 {
        last_push_id
    } else {
        new_rows[selected_new - 1].id
    };
    let has_more = selected_new < new_limit || new_rows.len() > MAX_NEW_EVENTS_PER_PACKET;
    let payload_bytes = serialize_payload(&state, &events)?;
    let now_secs = crate::shared::time::now_epoch_f64() as u64;
    let sealed = crypto::seal(psk, relay_id, &topic, &payload_bytes, now_secs)
        .map_err(|e| format!("seal: {e}"))?;
    let packet_bytes = ensure_complete_publish_packet_size(&topic, &sealed)?;

    Ok(PreparedPush {
        topic,
        sealed,
        packet_bytes,
        event_count: events.len(),
        max_event_id,
        has_more,
    })
}

pub(crate) fn record_queued(db: &HcomDb, prepared: &PreparedPush) {
    set_relay_status(db, "queued", None, true);
    log::log_with_fields(
        "INFO",
        "relay",
        "relay.push_queued",
        "",
        &[
            ("events", &prepared.event_count.to_string()),
            ("packet_bytes", &prepared.packet_bytes.to_string()),
            ("cursor_candidate", &prepared.max_event_id.to_string()),
        ],
    );
}

pub(crate) fn commit_broker_confirmed(db: &HcomDb, prepared: &PreparedPush) {
    let now = crate::shared::time::now_epoch_f64();
    safe_kv_set(db, "relay_last_push", Some(&now.to_string()));
    safe_kv_set(
        db,
        "relay_last_push_id",
        Some(&prepared.max_event_id.to_string()),
    );
    safe_kv_set(db, "relay_last_sync", Some(&now.to_string()));
    set_relay_status(db, "ok", None, true);
    log::log_with_fields(
        "INFO",
        "relay",
        "relay.push_broker_confirmed",
        "",
        &[
            ("events", &prepared.event_count.to_string()),
            ("packet_bytes", &prepared.packet_bytes.to_string()),
            ("cursor", &prepared.max_event_id.to_string()),
        ],
    );
}

/// Parse ISO 8601 timestamp to Unix epoch seconds.
fn parse_iso_timestamp_to_epoch(ts: &str) -> Option<f64> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .or_else(|_| chrono::DateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%SZ"))
        .ok()
        .map(|dt| dt.timestamp() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::HcomDb;
    use serde_json::json;

    const TEST_PSK: [u8; 32] = [0x41; 32];

    fn decoded_events(prepared: &PreparedPush) -> Vec<Value> {
        let plaintext =
            crypto::open(&TEST_PSK, "relay-test", &prepared.topic, &prepared.sealed).unwrap();
        let payload: Value = serde_json::from_slice(&plaintext).unwrap();
        payload["events"].as_array().unwrap().clone()
    }

    #[test]
    fn test_parse_iso_timestamp_to_epoch() {
        // RFC 3339
        let ts = parse_iso_timestamp_to_epoch("2024-01-01T00:00:00+00:00");
        assert!(ts.is_some());
        assert!(ts.unwrap() > 0.0);

        // Simple ISO format
        let ts = parse_iso_timestamp_to_epoch("2024-01-01T00:00:00Z");
        assert!(ts.is_some());

        // Invalid
        assert!(parse_iso_timestamp_to_epoch("not a date").is_none());
    }

    #[test]
    fn prepared_push_includes_recent_retained_tail() {
        let dir = tempfile::tempdir().unwrap();
        let db = HcomDb::open_at(&dir.path().join("hcom.db")).unwrap();

        let old_id = db
            .log_event("message", "old", &json!({"text": "old"}))
            .unwrap();
        let recent_id = db
            .log_event("message", "recent", &json!({"text": "recent"}))
            .unwrap();
        safe_kv_set(&db, "relay_last_push_id", Some(&recent_id.to_string()));

        let prepared = prepare_push(&db, "relay-test", "device-a", &TEST_PSK).unwrap();
        let events = decoded_events(&prepared);

        assert!(!prepared.has_more);
        assert_eq!(prepared.max_event_id, recent_id);
        assert!(
            events
                .iter()
                .any(|event| event["id"].as_i64() == Some(old_id)),
            "retained snapshot should include recent already-pushed events"
        );
        assert!(
            events
                .iter()
                .any(|event| event["id"].as_i64() == Some(recent_id))
        );
    }

    #[test]
    fn complete_packet_limit_includes_mqtt_framing() {
        let topic = "relay-test/device-a";
        let sealed = vec![0_u8; MAX_RELAY_PACKET_BYTES - 1];

        assert!(sealed.len() < MAX_RELAY_PACKET_BYTES);
        let err = ensure_complete_publish_packet_size(topic, &sealed).unwrap_err();
        assert!(err.contains("PUBLISH packet"));
        assert!(err.contains(&MAX_RELAY_PACKET_BYTES.to_string()));
    }

    #[test]
    fn adaptive_batches_drain_large_backlog_without_omitting_new_events() {
        let dir = tempfile::tempdir().unwrap();
        let db = HcomDb::open_at(&dir.path().join("hcom.db")).unwrap();
        let mut expected = Vec::new();
        for index in 0..30 {
            expected.push(
                db.log_event(
                    "message",
                    "sender",
                    &json!({"index": index, "text": "x".repeat(9_000)}),
                )
                .unwrap(),
            );
        }

        let mut observed = Vec::new();
        let mut packet_count = 0;
        loop {
            let cursor_before: i64 = safe_kv_get(&db, "relay_last_push_id")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            let prepared = prepare_push(&db, "relay-test", "device-a", &TEST_PSK).unwrap();
            packet_count += 1;
            assert!(prepared.packet_bytes <= MAX_RELAY_PACKET_BYTES);
            for event in decoded_events(&prepared) {
                let id = event["id"].as_i64().unwrap();
                if id > cursor_before {
                    observed.push(id);
                }
            }
            let has_more = prepared.has_more;
            commit_broker_confirmed(&db, &prepared);
            if !has_more {
                break;
            }
        }

        assert_eq!(
            packet_count, 3,
            "confirmed tail must not be repacked beside every new event"
        );
        assert_eq!(observed, expected);
        assert_eq!(
            safe_kv_get(&db, "relay_last_push_id").as_deref(),
            expected.last().map(ToString::to_string).as_deref()
        );
        assert_eq!(safe_kv_get(&db, "relay_status").as_deref(), Some("ok"));
    }

    #[test]
    fn single_oversized_event_fails_without_advancing_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let db = HcomDb::open_at(&dir.path().join("hcom.db")).unwrap();
        let event_id = db
            .log_event(
                "message",
                "sender",
                &json!({"text": "x".repeat(MAX_RELAY_PACKET_BYTES + 8_192)}),
            )
            .unwrap();

        let err = prepare_push(&db, "relay-test", "device-a", &TEST_PSK).unwrap_err();
        assert!(err.contains(&format!("relay event {event_id}")));
        assert!(err.contains("cursor remains 0"));
        assert!(safe_kv_get(&db, "relay_last_push_id").is_none());
        assert!(safe_kv_get(&db, "relay_last_push").is_none());
    }
}
