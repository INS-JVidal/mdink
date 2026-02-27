# Code Block Test Cases

## Rust

```rust
fn main() {
    let greeting = "Hello, world!";
    println!("{greeting}");

    for i in 0..5 {
        println!("Count: {i}");
    }
}
```

## Python

```python
def fibonacci(n):
    a, b = 0, 1
    for _ in range(n):
        yield a
        a, b = b, a + b

for num in fibonacci(10):
    print(num)
```

## JavaScript

```javascript
const fetchData = async (url) => {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`);
  }
  return response.json();
};
```

## Unknown Language

```nosuchlanguage
this should render as plain text
no crash expected
```

## Empty Fence (no language)

```
plain code block
without a language tag
```

## Empty Code Block

```
```

## Long Lines (should not wrap)

```rust
let very_long_variable_name_that_exceeds_terminal_width = "this is a very long string that should scroll horizontally rather than wrapping to the next line because code blocks preserve literal formatting";
```

## Indented Code Block

    This is an indented code block.
    It uses 4-space indentation.
    No language tag is possible here.

## Code Block Followed by Text

```rust
let x = 42;
```

This paragraph comes after a code block. Both should render correctly.
