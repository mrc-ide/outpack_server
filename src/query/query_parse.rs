use crate::query::query_types::*;
use crate::query::QueryError;
use lazy_static::lazy_static;
use pest::Parser;
use regex::Regex;
use crate::metadata::ParameterValue;

#[derive(Parser)]
#[grammar = "query/query.pest"]
struct QueryParser;

pub fn parse_query(query: &str) -> Result<QueryNode, QueryError> {
    match QueryParser::parse(Rule::query, query) {
        Ok(pairs) => {
            // Below never fails as query has been parsed and we know query rule can only have 1
            // expr and its inner can only be length 1 also (either latest or a string)
            let query = get_first_inner_pair(pairs.peek().unwrap());
            parse_query_content(query)
        }
        Err(e) => Err(QueryError::ParseError(Box::new(e))),
    }
}

fn parse_query_content(query: pest::iterators::Pair<Rule>) -> Result<QueryNode, QueryError> {
    match query.as_rule() {
        Rule::string => {
            let x = get_string_inner(query);
            Ok(QueryNode::Lookup(LookupLhs::Id, x))
        }
        Rule::noVariableFunc => {
            // Pest asserts for us that the only possible no variable func is latest()
            // we might want to move this validation into Rust code later to return
            // better errors to the user
            Ok(QueryNode::Latest(None))
        }
        Rule::infixExpression => {
            // Note that unwrap here is idiomatic pest code.
            // We can rely on the grammar to know that we can unwrap here, otherwise
            // it would have errored during pest parsing. See
            // https://pest.rs/book/parser_api.html#using-pair-and-pairs-with-a-grammar
            let mut infix = query.into_inner();
            let first_arg = infix.next().unwrap();
            let infix_function = infix.next().unwrap();
            let second_arg = infix.next().unwrap();
            match infix_function.as_str() {
                "==" => {
                    let lhs = get_first_inner_pair(first_arg);
                    let lookup_type = match lhs.as_rule() {
                        Rule::lookupId => LookupLhs::Id,
                        Rule::lookupName => LookupLhs::Name,
                        Rule::lookupParam => {
                            LookupLhs::Parameter(get_first_inner_pair(lhs).as_str())
                        }
                        _ => unreachable!(),
                    };
                    println!("{:?}", second_arg);
                    let rhs = get_first_inner_pair(second_arg);
                    match rhs.as_rule() {
                        Rule::string => {
                            let search_term = ParameterValue::String(get_string_inner(rhs));
                            Ok(QueryNode::Lookup(lookup_type, search_term))
                        },
                        Rule::integer => {
                            let search_term = ParameterValue::Integer(rhs.as_str());
                            Ok(QueryNode::Lookup(lookup_type, search_term))
                        },
                        Rule::float => {
                            let search_term = ParameterValue::Float(rhs.as_str());
                            Ok(QueryNode::Lookup(lookup_type, search_term))
                        },
                        Rule::bool => {
                            let search_term = ParameterValue::Bool(rhs.as_str());
                            Ok(QueryNode::Lookup(lookup_type, search_term))
                        },
                        _ => unreachable!(),
                    }
                }
                _ => {
                    let err = pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError {
                            message: format!(
                                "Encountered unknown infix operator: {}",
                                infix_function.as_str()
                            ),
                        },
                        infix_function.as_span(),
                    );
                    Err(QueryError::ParseError(Box::new(err)))
                }
            }
        }
        Rule::singleVariableFunc => {
            let mut func = query.into_inner();
            let func_name = func.next().unwrap().as_str();
            let arg = func.next().unwrap();
            let node_type = match func_name {
                "latest" => QueryNode::Latest,
                _ => unreachable!(),
            };
            let inner = parse_query_content(get_first_inner_pair(arg))?;
            Ok(node_type(Some(Box::new(inner))))
        }
        _ => unreachable!(),
    }
}

fn get_string_inner(rule: pest::iterators::Pair<Rule>) -> &str {
    get_first_inner_pair(rule).as_str()
}

fn get_first_inner_pair(rule: pest::iterators::Pair<Rule>) -> pest::iterators::Pair<Rule> {
    rule.into_inner().peek().unwrap()
}

// The interface accepts queries like `latest` and
// `"1234556"` which are not valid queries when used
// inside another query function. Before parsing with
// pest we preprocess these into their inferred full query
// e.g. `latest` -> `latest()`
// `"1234"` -> `id == "1234"`
pub fn preparse_query(query: &str) -> String {
    lazy_static! {
        static ref RE: Regex = Regex::new("^\"[A-Za-z0-9]*\"$").unwrap();
    }
    if query == "latest" {
        String::from("latest()")
    } else if RE.is_match(query) {
        format!("id == {}", query)
    } else {
        String::from(query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_can_be_preparsed() {
        let res = preparse_query("latest");
        assert_eq!(res, "latest()");
        let res = preparse_query("latest()");
        assert_eq!(res, "latest()");
        let res = preparse_query("latest(name == \"foo\")");
        assert_eq!(res, "latest(name == \"foo\")");
        let res = preparse_query("\"123\"");
        assert_eq!(res, "id == \"123\"");
        let res = preparse_query("name == \"foo\"");
        assert_eq!(res, "name == \"foo\"");
    }

    #[test]
    fn query_can_be_parsed() {
        let res = parse_query("latest()").unwrap();
        assert!(matches!(res, QueryNode::Latest(None)));
        let res = parse_query("id == \"123\"").unwrap();
        assert!(matches!(res, QueryNode::Lookup(LookupLhs::Id, "123")));
        let res = parse_query("id == \"12 3\"").unwrap();
        assert!(matches!(res, QueryNode::Lookup(LookupLhs::Id, "12 3")));
        let res = parse_query("name == \"123\"").unwrap();
        assert!(matches!(res, QueryNode::Lookup(LookupLhs::Name, "123")));
        let res = parse_query("parameter:x == \"foo\"").unwrap();
        assert!(matches!(res, QueryNode::Lookup(LookupLhs::Parameter("x"), "foo")));
        let res = parse_query("parameter:x==\"foo\"").unwrap();
        assert!(matches!(res, QueryNode::Lookup(LookupLhs::Parameter("x"), "foo")));
        let res = parse_query("parameter:longer==\"foo\"").unwrap();
        assert!(matches!(res, QueryNode::Lookup(LookupLhs::Parameter("longer"), "foo")));
        let res = parse_query("parameter:x123==\"foo\"").unwrap();
        assert!(matches!(res, QueryNode::Lookup(LookupLhs::Parameter("x123"), "foo")));
        let res = parse_query("latest(id == \"123\")").unwrap();
        match res {
            QueryNode::Latest(Some(value)) => {
                assert!(matches!(*value, QueryNode::Lookup(LookupLhs::Id, "123")))
            }
            _ => panic!("Invalid type, expected a QueryNode::Latest(Some(_))"),
        }
        let res = parse_query("latest(name == \"example\")").unwrap();
        match res {
            QueryNode::Latest(Some(value)) => assert!(matches!(
                *value,
                QueryNode::Lookup(LookupLhs::Name, "example")
            )),
            _ => panic!("Invalid type, expected a QueryNode::Latest(Some(_))"),
        }

        let res = parse_query("latest(\"123\")");
        match res {
            Ok(_) => panic!("Invalid query should have errored"),
            Err(e) => assert!(matches!(e, QueryError::ParseError(..))),
        };
        let res = parse_query("123");
        match res {
            Ok(_) => panic!("Invalid query should have errored"),
            Err(e) => assert!(matches!(e, QueryError::ParseError(..))),
        };
        let res = parse_query("name != \"123\"");
        match res {
            Ok(_) => panic!("Invalid query should have errored"),
            Err(e) => {
                assert!(matches!(e, QueryError::ParseError(..)));
                assert!(e
                    .to_string()
                    .contains("Encountered unknown infix operator: !="));
            }
        };
    }
}
