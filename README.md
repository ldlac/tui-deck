# tui-deck

A terminal-based slide deck presenter that renders Markdown files as interactive presentations.

## Features

- **Markdown-based** — Write slides in plain Markdown, split with `---`
- **Marp-compatible** — Supports Marp front matter and directives
- **Syntax highlighting** — 100+ languages via syntect (GitHub Dark theme)
- **ASCII art** — Preserves whitespace exactly in ` ```ascii ` blocks
- **Presenter notes** — Two syntaxes: `<!-- notes: -->` or `???` blocks
- **Dual-window mode** — Open presenter console with current/next slide + notes

## Installation

```bash
# Clone and build
git clone https://github.com/ldlac/tui-deck
cd tui-deck
cargo build --release

# Run
./target/release/tui-deck slides.md
```

## Usage

```bash
tui-deck [OPTIONS] <FILE>

Arguments:
  <FILE>  Markdown file to present [default: slides.md]

Options:
  --presenter  Open presenter console in second window
  --socket     Unix socket path for IPC [default: /tmp/tui-deck.sock]
  -h, --help  Print help
```

## Keyboard Controls

| Key               | Action           |
| ----------------- | ---------------- |
| `j` / `Space`    | Next slide       |
| `k` / `Backspace` | Previous slide   |
| `h`               | Previous slide   |
| `l`               | Next slide       |
| `←` / `→`         | Arrow navigation |
| `q`               | Quit             |

## Dual-Window Presenter Mode

Run the presentation in one terminal, then connect a presenter console in another:

```bash
# Terminal 1: Start the presentation
./target/release/tui-deck slides.md

# Terminal 2: Connect as presenter console
./target/release/tui-deck slides.md --presenter --socket /tmp/tui-deck.sock
```

The presenter window shows:
- Current slide (large)
- Next slide preview (smaller)
- Presenter notes
- Timer

Slide changes sync automatically via Unix socket.

## Markdown Format

### Basic Structure

```markdown
# Slide Title

Some content here.

---

## Next Slide

- Bullet point 1
- Bullet point 2
```

### Front Matter

```yaml
---
marp: true
theme: default
paginate: true
class: invert
backgroundColor: #1a1a2e
---
```

### Directives

```markdown
<!-- class: lead -->       <!-- Apply class to NEXT slide -->
<!-- _class: invert -->    <!-- Apply class to CURRENT slide -->
<!-- bg: #ff0000 -->       <!-- Background color -->
<!-- paginate: true -->    <!-- Show page number -->
```

### Presenter Notes

```markdown
<!-- notes: Your notes here -->

# Slide

???
Multiline
notes here
???
```

### Code Blocks

```rust
fn main() {
    println!("Hello, terminal!");
}
```

```python
def hello():
    print("Hello, world!")
```

### ASCII Art

```ascii
    ┌─────────────┐
    │   RUST      │
    └─────────────┘
```

### Image Sizing

```markdown
![width:200px](image.png)
![height:100px](logo.png)
```

## Example Presentation

````markdown
---
marp: true
paginate: true
theme: default
---

# Welcome

A terminal slide deck presenter

<!-- notes: Welcome to the demo! -->

---

## Features

- Markdown-based
- Syntax highlighting
- Presenter notes

---

## Code Example

```rust
fn main() {
    println!("Hello!");
}
```
````

<!-- notes: Explain the code here -->

---

# Thank You!

```

## Architecture

```

src/
├── main.rs # Entry point, TUI loop, event handling
├── parser.rs # Markdown → Slide AST
└── renderer.rs # Slide AST → Terminal rendering

```

- **parser.rs**: Uses pulldown-cmark for Markdown parsing, extracts Marp directives
- **renderer.rs**: Uses ratatui for terminal UI, syntect for syntax highlighting

## Requirements

- Rust 1.70+
- Terminal with 256-color support

## License

MIT
```
