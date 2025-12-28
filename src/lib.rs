//! groqfmt - Format GROQ queries with adaptive line wrapping.
//!
//! This library provides functionality to parse and format GROQ queries
//! using Wadler's "A prettier printer" algorithm for adaptive line wrapping.
//!
//! # Example
//!
//! ```
//! use groqfmt::format_query;
//!
//! let query = r#"*[_type == "article"]{ title, author->name }"#;
//! let formatted = format_query(query, 80).unwrap();
//! println!("{}", formatted);
//! ```

mod doc;
mod format;

pub use doc::Doc;
pub use format::format_expr;
use groq_parser::parser::Parser;

/// Format a GROQ query string with the given maximum line width.
///
/// # Arguments
///
/// * `query` - The GROQ query string to format
/// * `width` - Maximum line width for wrapping
///
/// # Returns
///
/// The formatted query string, or an error if parsing fails.
///
/// # Example
///
/// ```
/// use groqfmt::format_query;
///
/// let formatted = format_query("*[_type==\"post\"]{title}", 80).unwrap();
/// assert_eq!(formatted, "*[_type == \"post\"] { title }");
/// ```
pub fn format_query(query: &str, width: usize) -> Result<String, FormatError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(FormatError::EmptyQuery);
    }

    let mut parser = Parser::new(query);
    let tree = parser.parse().map_err(|e| FormatError::Parse(e.to_string()))?;

    let doc = format_expr(&tree);
    Ok(doc::pretty(width, doc))
}

/// Errors that can occur during formatting.
#[derive(Debug, Clone)]
pub enum FormatError {
    /// The query string was empty.
    EmptyQuery,
    /// Failed to parse the query.
    Parse(String),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatError::EmptyQuery => write!(f, "no query provided"),
            FormatError::Parse(msg) => write!(f, "parse error: {}", msg),
        }
    }
}

impl std::error::Error for FormatError {}

/// Default line width for formatting.
pub const DEFAULT_WIDTH: usize = 80;
