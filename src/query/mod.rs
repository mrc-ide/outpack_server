mod query_eval;
mod query_format;
mod query_parse;
mod query_types;

mod test_utils_query;

extern crate pest;

use crate::index::get_packet_index;
use crate::query::query_eval::eval_query;
use crate::query::query_format::format_query_result;
use crate::query::query_parse::Rule;

pub use crate::query::query_parse::parse_query;

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

#[derive(thiserror::Error, Debug, Clone)]
// Results with QueryError are at least as large as the QueryError variant. The compiler
// will need to reserve that much memory every time it is used. We want to keep this as
// small as possible so Box the large error body to force it onto the heap.
// See https://rust-lang.github.io/rust-clippy/master/index.html#result_large_err
pub enum QueryError {
    #[error("Failed to parse query\n{0}")]
    ParseError(Box<pest::error::Error<Rule>>),

    #[error("Failed to evaluate query\n{0}")]
    EvalError(String),
}
