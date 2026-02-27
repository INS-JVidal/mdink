# Font Slot Rendering Test

This file exercises the four ANSI font slots that mdink maps to markdown elements.
Configure your terminal with distinct fonts per slot to see the full effect:

  Normal = body text | Bold = h1-h3, strong | Italic = links, emphasis | Bold+Italic = h4-h6, inline code

---

## Slot 1: Normal (no modifier)

Plain body text renders with no ANSI modifiers. This is your terminal's default
font. Everything that is not a heading, link, emphasis, strong, or inline code
lands here.

---

## Slot 2: Bold

Headings h1 through h3 use the bold slot. So does **strong text** inside
paragraphs. If your terminal maps bold to a separate font family (e.g. a heavier
weight), these elements will render in that face.

# Heading 1 — Bold, LightCyan

## Heading 2 — Bold, Green

### Heading 3 — Bold, Yellow

---

## Slot 3: Italic

*Emphasized text* uses the italic slot. So do [hyperlinks](https://example.com)
and [links with **bold** inside](https://example.com). If your terminal maps
italic to a distinct font (e.g. a serif or cursive face), links and emphasis
will render in it.

Here is *a longer italic passage that should wrap across multiple lines to verify
that the italic modifier survives the word-wrap pipeline in layout.rs*.

A sentence with [a link](https://example.com) in the middle and *italic* nearby
to confirm they share the same font slot.

---

## Slot 4: Bold+Italic

#### Heading 4 — Bold+Italic, White

##### Heading 5 — Bold+Italic, White

###### Heading 6 — Bold+Italic, White

Inline code also uses the bold+italic slot: `fmt::Display`, `Vec<T>`, `unwrap()`.

A sentence mixing slots: **bold**, *italic*, `code`, and plain text all adjacent.

---

## Combined / Nested Formatting

**Bold with *italic nested* inside** uses bold for the outer, then bold+italic
for the nested portion.

*Italic with **bold nested** inside* uses italic for the outer, then bold+italic
for the nested portion.

***Bold and italic together*** lands in the bold+italic slot directly.

A [link containing **bold text**](https://example.com) should render the bold
text as bold+italic (italic from the link, bold from strong).

A [link containing *italic text*](https://example.com) should render as italic
(both link and emphasis map to the same slot — idempotent).

---

## Code Blocks with Comment Italics

Comments in code blocks are forced italic via the color-matching heuristic:

```rust
// This comment should render in the italic font slot.
fn main() {
    // Another comment — also italic.
    let x = 42; // Trailing comment — italic portion only.
    println!("Hello, world!"); // String and keyword stay non-italic.
}
```

```python
# Python comment — should also be italic.
def greet(name):
    """Docstring — may or may not match comment color depending on theme."""
    print(f"Hello, {name}!")
```

```javascript
// JS single-line comment — italic.
/* Block comment — italic if same color as line comments. */
const x = "not a comment"; // but this trailing part is.
```

Plain code block with no language (no syntax highlighting, no comment detection):

```
This is plain text in a code block.
No highlighting, no italic forcing.
```

---

## Edge Cases

An empty inline code span: `` — should still get bold+italic style.

Multiple inline code spans adjacent: `one` `two` `three` — each independently
styled in the bold+italic slot.

A heading with inline code: see below.

#### The `Config` struct — h4 heading

The heading text is bold+italic, and the inline code bypasses the heading style
stack (gets its own bg/fg + bold+italic).

A paragraph with every inline element:
**bold**, *italic*, ~~strikethrough~~, `code`, [link](https://example.com),
and ***bold italic*** — all in one line.

---

## Wrap Stress Test

This paragraph has a [very long link text that should wrap across multiple terminal lines to verify that the italic modifier applied by the link style stack entry is preserved through the byte-to-style map in wrap_styled_spans](https://example.com) and back to normal after.

Here is `a rather long inline code span that might wrap depending on terminal width` followed by normal text to verify style boundaries survive wrapping.
