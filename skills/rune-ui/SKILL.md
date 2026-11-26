---
name: rune-ui
description: "Write ```ui outlines that render as native frames, rows, chat threads, key-value facts, statuses, and controls in Brocode replies. Use when showing a UI mockup, product flow, before-and-after layout, settings screen, or status summary to a user in Brocode."
---

# Generative UI outlines (`ui`)

In Brocode, a ```` ```ui ```` code fence in a chat reply renders as native UI parts instead of a raw code block. Surround it with ordinary Markdown prose, headings, or code fences as needed.

## Rules

- Every non-blank line begins with a part name from the catalog below.
- Indent child lines by **2 spaces** per level (spaces only, never tabs; the first line is unindented).
- Keep outlines compact: at most **120** lines per outline, **4** nested container frames, and **32** words on one line.
- Never invent part names, pixel sizes, or hex colors. Color comes only from `{tone}` words:
  - **Done (green):** `{done}`, `{ship}`, `{merged}`, `{after}`, `{works}`, `{pass}`, `{fixed}`, `{live}`, `{landed}`, `{built}`, `{ok}`, `{yes}`, `{on}`
  - **Risk (red):** `{risk}`, `{blocked}`, `{broken}`, `{bug}`, `{fail}`, `{error}`, `{warning}`, `{danger}`, `{revert}`, `{no}`
  - **Proposal (amber):** `{proposal}`, `{hold}`, `{draft}`, `{plan}`, `{next}`, `{pending}`, `{waiting}`, `{todo}`, `{review}`, `{decide}`
  - **Working:** `{working}`, `{running}`, `{building}`
  - **Accent:** `{accent}`, `{primary}`, `{pick}`, `{recommended}`

## Parts catalog

| Syntax | Children | Draws |
| --- | --- | --- |
| `row` | any part | kids side by side; stacks on a narrow screen |
| `col` | any part | kids stacked, as one column of a row |
| `box [Title]` | any part | a quiet filled frame, its title a faint label |
| `screen [Title]` | any part | a desktop window around its kids |
| `phone [Title]` | any part | an iPhone-shaped frame around its kids |
| `arrow` | none | points from the kid before to the next |
| `title Words` | none | a short heading inside a frame |
| `text Words` | none | a sentence; Markdown, PRs and cards draw as chips |
| `item [{tone}] Label [· detail]` | none | a row with a status mark and a quiet detail |
| `kv Label: [{tone}] value [· detail]` | none | a label and its value, ruled like facts |
| `say Who [{how}]: words` | only `tools` | a chat line: you on the right, others left |
| `tools a, b` | none | the tools the chat line above used |
| `from Name` | none | who a phone thread is from, as its header |
| `toggle {on\|off} Label` | none | a switch, right-aligned beside its label |
| `button [{primary}] Label` | none | a pill button |
| `field [Placeholder]` | none | a text field with its placeholder |
| `chips a, b, c` | none | small labels in a row |
| `rule` | none | a hairline between kids |

## Examples by part

### `row` — kids side by side; stacks on a narrow screen

```ui
row
  box Before
    text One grammar per fence
  arrow
  box After
    text One outline
```

### `col` — kids stacked, as one column of a row

```ui
row
  col
    title Left
    text Stacked under it
  col
    title Right
    text Also stacked
```

### `box [Title]` — a quiet filled frame, its title a faint label

```ui
box Status
  item {done} Parser · landed
  item {working} Views · on a branch
```

### `screen [Title]` — a desktop window around its kids

```ui
screen Hex settings
  title Microphone
  toggle {on} Skip a closed laptop's mic
```

### `phone [Title]` — an iPhone-shaped frame around its kids

```ui
phone iPhone
  from Harbor
  say Alex {voice}: filter Lumen to Hex
  field Message
```

### `arrow` — points from the kid before to the next

```ui
row
  box Agent
  arrow
  box Lumen
```

### `title Words` — a short heading inside a frame

```ui
box
  title Microphone
```

### `text Words` — a sentence; Markdown, PRs and cards draw as chips

```ui
box
  text The closed laptop's mic drops out of the list.
```

### `item [{tone}] Label [· detail]` — a row with a status mark and a quiet detail

```ui
box
  item {done} Studio Display mic · in use
  item {risk} MacBook Pro mic · lid closed
```

### `kv Label: [{tone}] value [· detail]` — a label and its value, ruled like facts

```ui
box
  kv Status: {proposal} Nothing built · yet
  kv Owner: #108
```

### `say Who [{how}]: words` — a chat line: you on the right, others left

```ui
screen Desktop
  say Alex {voice}: filter Lumen to Hex
  say build {claude-opus-5-5}: Filtered. #96 waits on a pick.
    tools harbor, lumen
```

### `tools a, b` — the tools the chat line above used

```ui
screen
  say build: Done.
    tools harbor, lumen
```

### `from Name` — who a phone thread is from, as its header

```ui
phone
  from Harbor
```

### `toggle {on|off} Label` — a switch, right-aligned beside its label

```ui
screen Settings
  toggle {on} Skip a closed laptop's mic
  toggle {off} Ask before switching
```

### `button [{primary}] Label` — a pill button

```ui
box
  button {primary} Ship
  button Hold
```

### `field [Placeholder]` — a text field with its placeholder

```ui
phone
  field Message
```

### `chips a, b, c` — small labels in a row

```ui
box
  chips voice, spoken, agent
```

### `rule` — a hairline between kids

```ui
box
  text Above
  rule
  text Below
```

## Checking an outline

Validate a reply or standalone outline from the command line (`0` when clean, `1` with line numbers and suggestions when invalid):

~~~~sh
opencode-desktop --check-ui - <<'EOF'
```ui
row
  box Before
    text One grammar per fence
  arrow
  box After
    text One outline
```
EOF
~~~~
