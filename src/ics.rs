// SPDX-License-Identifier: CC0-1.0
// SPDX-FileCopyrightText: none
//! Parsing `.ics` files and building minimal single-event iCalendar exports for QR codes.
//!
//! Phone cameras expect compact, raw `VCALENDAR` text in the QR payload. This module
//! strips non-essential fields from the source file while preserving everything
//! needed to add an event (including recurrence rules when present).

use ical::generator::Emitter;
use ical::parser::Component;
use ical::parser::ical::component::{IcalCalendar, IcalEvent, IcalTimeZone};
use ical::property::Property;
use ical::IcalParser;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Core event fields copied into every exported QR payload.
const EVENT_FIELDS: &[&str] = &[
    "SUMMARY", "DTSTART", "DTEND", "DURATION", "LOCATION", "DESCRIPTION",
];

/// Recurrence-related fields copied when present on the source event.
const RECURRENCE_FIELDS: &[&str] = &["RRULE", "RDATE", "EXDATE", "EXRULE"];

const DESCRIPTION_LIMIT: usize = 120;
const LOCATION_LIMIT: usize = 80;

/// A parsed calendar file plus human-readable labels for the event picker.
pub struct LoadedCalendar {
    pub calendar: IcalCalendar,
    pub labels: Vec<String>,
}

/// Read and parse an iCalendar file from disk.
pub fn load_calendar(path: &Path) -> Result<LoadedCalendar, String> {
    let file = File::open(path).map_err(|err| format!("Could not open file: {err}"))?;
    let reader = BufReader::new(file);
    let mut parser = IcalParser::new(reader);

    let calendar = parser
        .next()
        .ok_or_else(|| "File does not contain a calendar".to_string())?
        .map_err(|err| format!("Failed to parse calendar: {err}"))?;

    if calendar.events.is_empty() {
        return Err("No events found in this calendar file".to_string());
    }

    let labels = calendar
        .events
        .iter()
        .enumerate()
        .map(|(index, event)| event_label(event, index))
        .collect();

    Ok(LoadedCalendar { calendar, labels })
}

/// Build a minimal single-event `.ics` string suitable for encoding in a QR code.
pub fn event_ics(calendar: &IcalCalendar, event_index: usize) -> Result<String, String> {
    let event = calendar
        .events
        .get(event_index)
        .ok_or_else(|| "Selected event no longer exists".to_string())?;

    let minimal_event = minimal_event(event, event_index);
    let mut single = IcalCalendar::new();
    single.add_property(property("VERSION", "2.0"));
    single.add_property(property("PRODID", "-//i//EN"));
    single.timezones = referenced_timezones(calendar, &minimal_event);
    single.events = vec![minimal_event];

    Ok(single.generate())
}

/// Copy whitelisted properties from `source`, add recurrence rules, and ensure UID/DTSTAMP exist.
fn minimal_event(source: &IcalEvent, event_index: usize) -> IcalEvent {
    let mut event = IcalEvent::new();

    for name in EVENT_FIELDS {
        let Some(mut prop) = source
            .properties
            .iter()
            .find(|property| property.name == *name)
            .cloned()
        else {
            continue;
        };

        match prop.name.as_str() {
            "DESCRIPTION" => truncate_value(&mut prop, DESCRIPTION_LIMIT),
            "LOCATION" => truncate_value(&mut prop, LOCATION_LIMIT),
            _ => {}
        }

        event.add_property(prop);
    }

    copy_recurrence_properties(source, &mut event);

    let uid = property_value(source, "UID")
        .unwrap_or_else(|| format!("{event_index}@i"));
    event.add_property(property("UID", &uid));

    // DTSTAMP is required by RFC 5545; fall back to DTSTART when the source omits it.
    let stamp = property_value(source, "DTSTAMP")
        .or_else(|| property_value(&event, "DTSTART"))
        .unwrap_or_else(|| "19700101T000000Z".into());
    event.add_property(property("DTSTAMP", &stamp));

    event
}

/// Preserve all recurrence properties from the source (including multiple EXDATE lines).
fn copy_recurrence_properties(source: &IcalEvent, event: &mut IcalEvent) {
    for prop in &source.properties {
        if RECURRENCE_FIELDS.contains(&prop.name.as_str()) {
            event.add_property(prop.clone());
        }
    }
}

/// Include only the VTIMEZONE block that matches the event's TZID, if any.
fn referenced_timezones(calendar: &IcalCalendar, event: &IcalEvent) -> Vec<IcalTimeZone> {
    let Some(tzid) = tzid_from_event(event) else {
        return Vec::new();
    };

    calendar
        .timezones
        .iter()
        .filter(|timezone| timezone_tzid(timezone).as_deref() == Some(tzid.as_str()))
        .cloned()
        .collect()
}

fn tzid_from_event(event: &IcalEvent) -> Option<String> {
    for name in ["DTSTART", "DTEND"] {
        let Some(property) = event.properties.iter().find(|property| property.name == name) else {
            continue;
        };
        if let Some(tzid) = param_value(property, "TZID") {
            return Some(tzid);
        }
    }
    None
}

fn timezone_tzid(timezone: &IcalTimeZone) -> Option<String> {
    timezone
        .properties
        .iter()
        .find(|property| property.name == "TZID")
        .and_then(|property| property.value.clone())
}

fn truncate_value(property: &mut Property, max_len: usize) {
    let Some(value) = property.value.as_mut() else {
        return;
    };

    if value.chars().count() <= max_len {
        return;
    }

    let truncated: String = value.chars().take(max_len.saturating_sub(1)).collect();
    *value = format!("{truncated}…");
}

fn property(name: &str, value: &str) -> Property {
    Property {
        name: name.into(),
        params: None,
        value: Some(value.into()),
    }
}

fn param_value(property: &Property, name: &str) -> Option<String> {
    property.params.as_ref()?.iter().find_map(|(key, values)| {
        if key == name {
            values.first().cloned()
        } else {
            None
        }
    })
}

fn event_label(event: &IcalEvent, index: usize) -> String {
    let summary = property_value(event, "SUMMARY").unwrap_or_else(|| "(No title)".to_string());
    let start = property_value(event, "DTSTART").unwrap_or_default();

    if start.is_empty() {
        format!("{}. {summary}", index + 1)
    } else {
        format!("{}. {summary} ({start})", index + 1)
    }
}

fn property_value(event: &IcalEvent, name: &str) -> Option<String> {
    event
        .properties
        .iter()
        .find(|property| property.name == name)
        .and_then(|property| property.value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_ics_omits_alarms_and_extra_fields() {
        let mut event = IcalEvent::new();
        event.add_property(property("SUMMARY", "Meet"));
        event.add_property(property("DTSTART", "20260615T140000Z"));
        event.add_property(property("DTEND", "20260615T150000Z"));
        event.add_property(property("ORGANIZER", "mailto:boss@example.com"));
        event.add_property(property(
            "DESCRIPTION",
            "A".repeat(DESCRIPTION_LIMIT + 20).as_str(),
        ));

        let calendar = IcalCalendar {
            events: vec![event],
            ..Default::default()
        };

        let ics = event_ics(&calendar, 0).unwrap();
        assert!(ics.contains("SUMMARY:Meet"));
        assert!(!ics.contains("ORGANIZER"));
        assert!(!ics.contains("VALARM"));
        assert!(!ics.contains("METHOD"));
        assert!(ics.contains("PRODID:-//i//EN"));
        assert!(ics.contains("DESCRIPTION:"));
        assert!(!ics.contains(&"A".repeat(DESCRIPTION_LIMIT + 10)));
    }

    #[test]
    fn recurring_event_includes_rrule_and_exdate() {
        let mut event = IcalEvent::new();
        event.add_property(property("SUMMARY", "Standup"));
        event.add_property(property("DTSTART", "20260615T140000Z"));
        event.add_property(property("DTEND", "20260615T143000Z"));
        event.add_property(property("RRULE", "FREQ=WEEKLY;BYDAY=MO,WE,FR"));
        event.add_property(property("EXDATE", "20260701T140000Z"));
        event.add_property(property("EXDATE", "20260703T140000Z"));
        event.add_property(property("ATTENDEE", "mailto:a@example.com"));

        let calendar = IcalCalendar {
            events: vec![event],
            ..Default::default()
        };

        let ics = event_ics(&calendar, 0).unwrap();
        assert!(ics.contains("RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR"));
        assert!(ics.contains("EXDATE:20260701T140000Z"));
        assert!(ics.contains("EXDATE:20260703T140000Z"));
        assert!(!ics.contains("ATTENDEE"));
    }

    #[test]
    fn non_recurring_event_omits_rrule() {
        let mut event = IcalEvent::new();
        event.add_property(property("SUMMARY", "Once"));
        event.add_property(property("DTSTART", "20260615T140000Z"));
        event.add_property(property("DTEND", "20260615T150000Z"));

        let calendar = IcalCalendar {
            events: vec![event],
            ..Default::default()
        };

        let ics = event_ics(&calendar, 0).unwrap();
        assert!(!ics.contains("RRULE:"));
        assert!(!ics.contains("EXDATE:"));
    }
}
