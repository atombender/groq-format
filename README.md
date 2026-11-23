# groq-format

A formatter for [GROQ](https://www.sanity.io/docs/groq) queries with adaptive line wrapping.

The layout follows [Prettier](https://prettier.io): short expressions stay on one line, and longer ones break at the outermost point first.

- A filter `[...]` that does not fit puts its condition on its own indented line, like the condition of an `if`. If that is still too long, `&&` and `||` chains break with one operand per line and leading operators. A slice, projection or pipe that follows hugs the closing `]`.
- Pipelines behave like `&&` chains, all or nothing: if the whole chain fits after its head it stays on that line, and otherwise every `|` goes on a line of its own, indented, with a leading `|`. A single `|` after a multi-line head (`}[0...10] | order(_createdAt desc)`) stays when it fits. A trailing slice or projection hugs the last stage and breaks on its own. As a `key:` value, a pipeline that does not fit next to the key moves below it; as a call argument the stages are indented.
- Slices and indexes (`[0...10]`, `[0]`) never break.
- Projections `{ ... }` stay inline when they fit, and otherwise put one member per line.
- Calls whose last argument is an object, array, filter or projection hug it, as in `count(*[` … `])`.
- Comparison and arithmetic operators break (before the operator) only when nothing else makes the line fit.
- Comments are kept, and formatting is idempotent.

The layout agrees with the hand-written examples in the Sanity documentation: a broken filter has its condition on an indented line with `]` alone, `select()` and `score()` arguments go one per line with the closing `)` lined up, `=> { ... }` conditionals hug their projection, `fn` definitions hug `$arg {`, and pipe stages go one per line with a leading `|` when the chain does not fit. The output is a function of the query alone, so the documentation's expanded one-member projections, trailing commas, aligned trailing comments and single-quoted strings are normalised. The examples from the documentation are part of the test corpus (`tests/corpus/docs.txt`).

## Installation

```bash
cargo install --git https://github.com/sanity-io/groq-format
```

## Use with Zed

This extension can be used with [Zed](https://zed.dev/) by first installing [this GROQ extension](https://github.com/juice49/zed-groq) and then adding the following configuration to your `~/.config/zed/settings.json`. Make sure to install the command first using `cargo install` or similar.

```json
  "languages": {
    "GROQ": {
      "formatter": {
        "external": { "command": "groq-format", "arguments": [] },
      },
      "format_on_save": "on",
    },
  },
```

## CLI Usage

```bash
# Format a file to stdout
groq-format query.groq

# Format multiple files
groq-format query1.groq query2.groq

# Format file(s) in-place
groq-format -w query.groq

# Format from stdin
echo '*[_type == "article"]' | groq-format

# Set max line width (default: 80)
groq-format -W 120 query.groq
```

### Options

| Flag | Description |
|------|-------------|
| `-w, --write` | Write result back to source file instead of stdout |
| `-W, --width <WIDTH>` | Maximum line width (default: 80) |
| `--force-wrap` | Break before comparison and arithmetic operators as soon as an expression does not fit |
| `-h, --help` | Print help |
| `-V, --version` | Print version |

## Library Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
groq-format = { git = "https://github.com/sanity-io/groq-format" }
```

Then use in your code:

```rust
use groq_format::format_query;

fn main() {
    let query = r#"*[_type == "article" && published == true]{ title, author->name }"#;

    match format_query(query, 80) {
        Ok(formatted) => println!("{}", formatted),
        Err(e) => eprintln!("Error: {}", e),
    }
}
```

### API

#### `format_query(query: &str, width: usize) -> Result<String, FormatError>`

Parses and formats a GROQ query string with the given maximum line width.

#### `FormatError`

Error type returned when formatting fails:
- `FormatError::EmptyQuery` - The input query was empty
- `FormatError::Parse(String)` - Failed to parse the query

#### `DEFAULT_WIDTH`

The default line width constant (80).

## Example

Input:
```groq
*[_type == "article" && published == true && category in ["tech", "science"]]{ title, "slug": slug.current, author->{ name, image } }
```

Output:
```groq
*[_type == "article" && published == true && category in ["tech", "science"]] {
  title,
  "slug": slug.current,
  author->{ name, image }
}
```

## License

MIT License - see [LICENSE](LICENSE)
