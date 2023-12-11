mod query_eval;
mod query_format;
mod query_parse;
mod query_types;

mod test_utils_query;

#[cfg(feature = "python")]
mod python;

use crate::index::get_packet_index;
use crate::query::query_eval::eval_query;
use crate::query::query_format::format_query_result;
pub use crate::query::query_parse::parse_query;
use crate::query::query_parse::Rule;

use thiserror::Error;

pub fn run_query(root: &str, query: &str) -> Result<String, QueryError> {
    let index = match get_packet_index(root) {
        Ok(index) => index,
        Err(e) => {
            return Err(QueryError::EvalError(format!(
                "Could not build outpack index from root at {}: {:?}",
                root, e
            )))
        }
    };
    let parsed = parse_query(query)?;
    let result = eval_query(&index, parsed);
    format_query_result(result)
}

// pest's error type is quite large, which would consume a lot of stack space and require moving
// data around, even in the happy path when an Ok is returned. We want to keep this as small as
// possible so Box the large error body to force it onto the heap. The heap memory allocation cost
// is only incurred when an actual error is returned.
// See https://rust-lang.github.io/rust-clippy/master/index.html#result_large_err
#[derive(Error, Debug, Clone)]
#[error(transparent)]
pub struct ParseError(Box<pest::error::Error<Rule>>);

#[derive(Error, Debug, Clone)]
pub enum QueryError {
    #[error("Failed to parse query\n{0}")]
    ParseError(#[from] ParseError),

    #[error("Failed to evaluate query\n{0}")]
    EvalError(String),
}

impl From<pest::error::Error<Rule>> for ParseError {
    fn from(err: pest::error::Error<Rule>) -> ParseError {
        ParseError(Box::new(err))
    }
}
