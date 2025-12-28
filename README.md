# groq-format

A formatter for [GROQ](https://www.sanity.io/docs/groq) queries with adaptive line wrapping.

Uses Wadler's "A prettier printer" algorithm to intelligently wrap long lines while keeping short queries compact.

## Installation

```bash
cargo install --git https://github.com/sanity-io/groq-format
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
*[_type == "article"
    && published == true
    && category in ["tech", "science"]] {
  title,
  "slug": slug.current,
  author-> { name, image }
}
```

## License

MIT License - see [LICENSE](LICENSE)
