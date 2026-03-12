# Mapper Tool UI Design

## 1) Purpose

This document defines the UI design for the mapper tool, including:
- Wireframes of all primary screens/widgets
- UI states for each screen
- Transitions between screens/modal dialogs
- Behavioral expectations for key user actions

The target UI framework is Textual (terminal UI), implemented in `scripts/mappertool/mapper_tool_app.py`.

---

## 2) Information Architecture

The mapper tool has one primary screen with modal overlays.

- Primary Screen: Mapper Tool Dashboard
  - ROM Pane (left / top-left)
  - Actions Pane (right / top-right)
  - Logs Pane (bottom)
- Modal Overlays
  - Scan Progress Modal
  - ROM Command Modal
  - Autorun Run Status Modal

---

## 3) Global Layout Wireframe

<pre>
+---------------------------------------------------------------------------------------------------------------+
|                                         Neser Mappertool                                                      |
+---------------------------------------------------------------------------------------------------------------+
|                                                 TOP PANES                                                     |
| +-----------------------------------------------------------------------------------+ +---------------------+ |
| | ROMs                                                                              | | Actions             | |
| |-----------------------------------------------------------------------------------| |---------------------| |
| | [ROM name filter.................................] [Mapper filter...............] | | ROM Inventory       | |
| | [ ] Show only ROMs with autorun files                                               | | [rom root input....]| |
| |                                                                                   | | [Drop and rescan]   | |
| | +-------------------------------------------------------------------------------+ | | [Playback All ...]  | |
| | | Map | SMap | ROM                     | Autorun  | CRC       | Source         | | | [Playback Not run]  | |
| | |-----+------+-------------------------+----------+-----------+----------------| | | [Recalculate fail..]| |
| | | ... rows with full values visible without clipping                        ... | | |                     | |
| | +-------------------------------------------------------------------------------+ | +---------------------+ |
| +-----------------------------------------------------------------------------------+                         |
+---------------------------------------------------------------------------------------------------------------+
|                                                BOTTOM PANE                                                    |
| +-----------------------------------------------------------------------------------------------------------+ |
| | Logs                                                                                                      | |
| |-----------------------------------------------------------------------------------------------------------| |
| | Mappertool logs will appear here...                                                                       | |
| | ...                                                                                                       | |
| +-----------------------------------------------------------------------------------------------------------+ |
+---------------------------------------------------------------------------------------------------------------+
</pre>

Layout ratios:
- Top area split horizontally: ROM pane ~70%, Actions pane ~30% (wider ROM table)
- Bottom logs pane spans full width

Initial focus:
- Focus should land on ROM table at startup.

---

## 4) Screen/Widget Designs

## 4.1 Dashboard (Primary Screen)

### ROM Pane

Components:
- `name-filter-input` (left)
- `mapper-filter-input` (right)
- `autorun-only-filter` checkbox
- `rom-database` DataTable (row cursor)

Table columns:
1. Map
2. SMap
3. ROM
4. Autorun
5. CRC
6. Source

Autorun column values:
- `N/A` (no `.autorun` file)
- `Not run` (grey highlight)
- `FAIL` (red highlight)
- `PASS` (green highlight)

### Actions Pane

Components:
- ROM root input
- `Drop and rescan` button (warning/yellow)
- `Playback All Recordings` button (warning/yellow)
- `Playback Not run` button (warning/yellow)
- `Recalculate failing CRCs` button (warning/yellow)

### Logs Pane

Components:
- Read-only selectable `TextArea`
- Supports selecting and copying log text

---

## 4.2 Scan Progress Modal

Shown during inventory scan and rescan.

<pre>
+----------------------------------------------+
| Scanning ROM inventory...                    |
|                                              |
| [Cancel]                                     |
+----------------------------------------------+
</pre>

Behavior:
- Cancel requests scan cancellation.
- Modal closes when scan completes or is canceled.

---

## 4.3 ROM Command Modal

Opened by selecting a row in ROM table.

Header block:
- Row 1: `ROM Commands: <rom_filename>`
- Row 2 (only if autorun exists):
  - `Autorun: <#frames> frames, <#checkpoints> CRCs, <status>`

Status is one of: `N/A`, `Not run`, `FAIL`, `PASS`.

### Modal variant A: ROM without autorun

<pre>
+--------------------------------------------------------------+
| ROM Commands: demo.nes                                       |
|                                                              |
| [Create autorun recording]                                   |
| [Run ROM]                                                    |
| [Cancel]                                                     |
+--------------------------------------------------------------+
</pre>

### Modal variant B: ROM with autorun

<pre>
+--------------------------------------------------------------+
| ROM Commands: demo.nes                                       |
| Autorun: 1200 frames, 4 CRCs, PASS                           |
|                                                              |
| [Playback recording (headless)]                              |
| [Playback recording (headed)]                                |
| [Etended recording]                                          |
| [Run ROM]                                                    |
| [Recalculate CRCs]                                            |
| [Delete recording]                                           |
| [Cancel]                                                     |
+--------------------------------------------------------------+
</pre>

Button variants:
- All buttons warning/yellow except `Delete recording` (error/red).
- Dialog closes before command execution begins.

---

## 4.4 Autorun Run Status Modal

Shown during playback/create/extend command execution.

<pre>
+--------------------------------------------------------------+
| Building / Running status row                                |
| Full file-set progress row                                   |
| Current file progress row                                    |
|                                                              |
| [Cancel]                                                     |
+--------------------------------------------------------------+
</pre>

Rows:
1. Building/Running state
2. Progress of full file set (`File X/Y: <name>`) for batch runs
3. Progress within current file (`Checkpoint A/B. Errors: N`)

Behavior:
- Cancel terminates active subprocess.
- After cancel, a new batch run must be startable immediately.

---

## 5) Transition Design

## 5.1 High-Level State Machine

<pre>
[App Start]
    |
    v
[Dashboard Loading] --scan complete--> [Dashboard Ready]
       |                                   |
       +--scan cancel----------------------+

[Dashboard Ready] --row select--> [ROM Command Modal]
[Dashboard Ready] --drop/rescan--> [Scan Progress Modal]
[Dashboard Ready] --playback all/not run--> [Autorun Run Status Modal]

[ROM Command Modal] --Cancel--> [Dashboard Ready]
[ROM Command Modal] --Action--> [Autorun Run Status Modal]

[Scan Progress Modal] --complete/cancel--> [Dashboard Ready]
[Autorun Run Status Modal] --complete/cancel--> [Dashboard Ready]
</pre>

## 5.2 Transition Table

| From | Trigger | To | Notes |
|---|---|---|---|
| App Start | Auto mount | Dashboard Loading | Starts scan worker |
| Dashboard Loading | Scan done | Dashboard Ready | Table is populated |
| Dashboard Loading | Scan cancel | Dashboard Ready | Partial results allowed |
| Dashboard Ready | Select ROM row | ROM Command Modal | Whole row selected |
| Dashboard Ready | Drop and rescan | Scan Progress Modal | Inventory rebuilt |
| Dashboard Ready | Playback All Recordings | Autorun Run Status Modal | Plays all autorun files |
| Dashboard Ready | Playback Not run | Autorun Run Status Modal | Plays only Not run autorun files |
| Dashboard Ready | Recalculate failing CRCs | Autorun Run Status Modal | Recalculates CRCs for FAIL autorun files |
| ROM Command Modal | Cancel | Dashboard Ready | No action executed |
| ROM Command Modal | Any command button | Autorun Run Status Modal | Modal closes first |
| Autorun Run Status Modal | Cancel | Dashboard Ready | Terminates active process |
| Autorun Run Status Modal | Completion | Dashboard Ready | Status persisted to table |

---

## 6) Interaction Notes

- Sorting by headers should continue to work for all columns, including Autorun.
- Autorun status persistence source of truth is `rom_files.csv`.
- If autorun metadata cannot be parsed, dialog still shows summary with `unknown` values where needed.
- Row tooltip should continue to show full ROM path.

---

## 7) Non-Goals

- No additional screens beyond the dashboard + current modals.
- No changes to emulator core playback behavior.
- No theme redesign beyond current warning/error style conventions.

---

## 8) Future Extensions (Optional)

- Add explicit status legend below the table for Autorun colors.
- Add batch-run completion summary modal with pass/fail counts.
- Add retries for failed entries in batch playback.
