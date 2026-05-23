# ICS QR

A cross-platform desktop app (Linux and Windows) that turns a single iCalendar event into a
scannable QR code. Scan the code with a phone camera to add the event to Apple Calendar,
Google Calendar, Outlook, or any other app that understands iCalendar (`.ics`) data.

Built with [Rust](https://www.rust-lang.org/) and [Slint](https://slint.dev/).

## Features

- Open an `.ics` file from disk or drag-and-drop (X11 on Linux)
- Pick one event from a multi-event calendar export
- Generate a QR code containing a **minimal** iCalendar payload optimized for phone scanning
- Copy the QR image to the clipboard or save it as a PNG
- Responsive QR preview that scales with the window

## Prerequisites

- **Rust** 1.75 or newer (2024 edition)
- **Linux:** development libraries for Slint/winit, including `fontconfig`. On Debian/Ubuntu:

  ```bash
  sudo apt install libfontconfig-dev
  ```

  If `pkg-config` cannot find `fontconfig`, set:

  ```bash
  export PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig
  ```

- **Windows:** a standard Rust MSVC or GNU toolchain; no extra system packages are usually required.

## Build

```bash
cargo build --release
```

The binary is written to `target/release/icsqr`.

Run tests:

```bash
cargo test
```

## Usage

1. Start the app:

   ```bash
   cargo run --release
   ```

2. Click **Open ICS File** and choose an `.ics` / `.ical` export, or drag a file onto the window (Linux X11 only; see [Platform notes](#platform-notes)).
3. Select an event from the dropdown. A QR code is generated automatically.
4. Scan the QR with your phone’s camera. You should be prompted to **Add to Calendar** (iOS) or **Create event** (Android).
5. Optionally use **Copy Image** or **Save Image** to share the QR outside the app.

### Expected workflow

ICS QR is meant for sharing **one event** at a time in a physical or digital context where
a URL is awkward: posters, slide decks, name badges, printed handouts, or chat messages.
The QR encodes the event directly — no server, account, or internet connection is required
at scan time.

Each QR holds a self-contained `VCALENDAR` with a single `VEVENT`. The payload is **raw
iCalendar text**, not a `data:` URI or hosted link, because phone cameras reliably
recognize that format.

## QR payload format

The QR code contains plain iCalendar data, for example:

```ics
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//i//EN
BEGIN:VEVENT
SUMMARY:Team standup
DTSTART:20260615T140000Z
DTEND:20260615T143000Z
RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR
UID:abc123@google.com
DTSTAMP:20260615T120000Z
END:VEVENT
END:VCALENDAR
```

When scanned, the phone’s calendar app parses this text and offers to save the event.

## What is kept vs. stripped

To keep QR codes scannable, the app exports a **minimal** calendar — not a byte-for-byte
copy of the source file.

### Kept

| Scope | Fields |
| --- | --- |
| Calendar | `VERSION`, `PRODID` |
| Event | `SUMMARY`, `DTSTART`, `DTEND`, `DURATION`, `LOCATION`, `DESCRIPTION`, `UID`, `DTSTAMP` |
| Recurrence (when present) | `RRULE`, `RDATE`, `EXDATE`, `EXRULE` (all occurrences) |
| Timezone | Only the `VTIMEZONE` block referenced by the event’s `TZID`, if any |

`DESCRIPTION` is truncated to 120 characters and `LOCATION` to 80 characters.

### Stripped

These are omitted to reduce QR density and because phones rarely need them for
“add to calendar”:

- Alarms (`VALARM`)
- Attendees, organizer, categories, status, priority, class, transparency
- Conference URLs, attachments, geo coordinates, custom `X-` properties
- Calendar-level metadata from the source (`METHOD`, `CALSCALE`, long `PRODID`, etc.)
- Unused timezone definitions
- All other events in the file (only the selected event is exported)

If you need the full original event — including attendees, reminders, or proprietary
fields — share the `.ics` file directly instead of a QR code.

## Platform notes

| Feature | Linux (X11) | Linux (Wayland) | Windows |
| --- | --- | --- | --- |
| Open file dialog | Yes | Yes | Yes |
| Drag-and-drop | Yes | No | Yes |
| Copy / save QR | Yes | Yes | Yes |

On Wayland, file dialogs use the async portal API and are attached to the app window so the
compositor does not treat the UI as frozen. Drag-and-drop is not supported by winit on
Wayland; use **Open ICS File** instead.

## Project layout

```
icsqr/
├── build.rs          # Compiles ui/app.slint
├── ui/app.slint      # Slint UI definition
└── src/
    ├── main.rs       # Application entry point and UI wiring
    ├── ics.rs        # ICS parsing and minimal export
    └── qr.rs         # QR code generation
```

## Contributing

The original codebase for this project was 100% vibe coded. Contributions are welcome, but
I hold little personal investment in this project.

I will gladly credit contributors to this project for their contributions.

## Creative Commons Public Dedication

I do not believe AI-written code to be copyrightable, nor to have a creator in the legal
sense. In service to that belief, I am making it explicit with this project and entering it
into the public domain, foregoing any personal claim to copyright.

This project is not licensed. It is freely given under the Creative Commons Zero (CC0)
terms.

I (William Page) am the grantor, and disclaim copyright to all project assets, including
source code and metadata.

As such, no works in this project are under copyright, but are in the public domain.

See the GRANT.txt file in the project for the full legal text.
