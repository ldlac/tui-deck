---
marp: true
theme: default
paginate: true
class: invert
backgroundColor: #1a1a2e
---

# Marp-Compatible tui-deck

A terminal slide deck presenter

<!-- notes: Welcome everyone! Today I'll show you tui-deck with full Marp compatibility. -->

---

## Marp Front Matter

```yaml
---
marp: true
theme: default
paginate: true
class: invert
backgroundColor: #1a1a2e
---
```

- `marp: true` - Enable Marp mode
- `theme:` - Theme name
- `paginate:` - Show page numbers
- `class:` - Global CSS classes
- `backgroundColor:` - Global background

<!-- notes: The front matter goes at the very top of your markdown file. -->

---

## Slide Directives

<!-- class: lead -->

### Class Directives

- `<!-- class: lead -->` - Apply class to next slide
- `<!-- _class: lead -->` - Apply to current slide
- `<!-- bg: #ff0000 -->` - Background color
- `<!-- paginate: true -->` - Enable pagination

<!-- notes: Directives are HTML comments that control slide behavior. -->

---

## Presenter Notes

### Multiple Syntax Supported

1. `<!-- notes: Your notes here -->`
2.

```
???
Your notes here
???
```

Both work identically!

<!-- notes: Use presenter notes to add speaker notes to your slides. -->

---

## Code Highlighting

```rust
fn main() {
    let x = vec![1, 2, 3];
    for i in x.iter() {
        println!("{}", i);
    }
}
```

Works with 100+ languages!

---

## ASCII Art

```ascii
    ┌─────────────┐
    │   RUST      │
    │  ┌───────┐  │
    │  │  ★    │  │
    │  └───────┘  │
    └─────────────┘
```

Preserved exactly as written.

---

## Navigation

| Key               | Action     |
| ----------------- | ---------- |
| `j` / `Space`     | Next slide |
| `k` / `Backspace` | Previous   |
| `h`               | Previous   |
| `l`               | Next       |
| `←` `→`           | Arrow keys |
| `q`               | Quit       |

---

# Thank You!

GitHub: @ldlac/tui-deck
