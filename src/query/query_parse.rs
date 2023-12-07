use pest::iterators::{Pair, Pairs};
use pest::pratt_parser::PrattParser;
use pest::Parser;
use pest_derive::Parser;

use crate::query::query_types::*;
use crate::query::ParseError;

#[derive(Parser)]
#[grammar = "query/query.pest"]
struct QueryParser;

pub fn parse_query(query: &str) -> Result<QueryNode, ParseError> {
    let pairs = QueryParser::parse(Rule::query, query)?;
    let node = parse_toplevel(get_first_inner_pair(pairs.peek().unwrap()))?;
    Ok(node)
}

/// Parse the top-level syntax node.
///
/// The syntax allows a few short-form queries, like `latest` and `"123456"`
/// which are not valid expressions (ie. they cannot appear inside other query
/// functions).
///
/// This function handles these, and delegates any long-form query to the pratt
/// parser below.
fn parse_toplevel(toplevel: Pair<Rule>) -> Result<QueryNode, ParseError> {
    match toplevel.as_rule() {
        Rule::body => parse_body(toplevel.into_inner()),
        Rule::shortformLatest => Ok(QueryNode::Latest(None)),
        Rule::shortformId => {
            let id = get_string_inner(get_first_inner_pair(toplevel));
            let lhs = TestValue::Lookup(Lookup::Packet(PacketLookup::Id));
            let rhs = TestValue::Literal(Literal::String(id));
            Ok(QueryNode::Test(TestOperator::Equal, lhs, rhs))
        }
        _ => unreachable!(),
    }
}

lazy_static::lazy_static! {
    static ref PRATT_PARSER: PrattParser<Rule> = {
        use pest::pratt_parser::{Assoc::*, Op};
        use Rule::*;

        // Precedence is defined lowest to highest
        PrattParser::new()
            // And has higher index precedence
            .op(Op::infix(or, Left))
            .op(Op::infix(and, Left))
            .op(Op::prefix(negation))
    };
}

pub fn parse_body(pairs: Pairs<Rule>) -> Result<QueryNode, ParseError> {
    PRATT_PARSER
        .map_primary(parse_expr)
        .map_prefix(|op, rhs| match op.as_rule() {
            Rule::negation => Ok(QueryNode::Negation(Box::new(rhs?))),
            _ => unreachable!(),
        })
        .map_infix(|lhs, op, rhs| {
            let op = match op.as_rule() {
                Rule::and => BooleanOperator::And,
                Rule::or => BooleanOperator::Or,
                rule => unreachable!("Parse expected infix operation, found {:?}", rule),
            };
            Ok(QueryNode::BooleanExpr(op, Box::new(lhs?), Box::new(rhs?)))
        })
        .parse(pairs)
}

fn parse_expr(query: Pair<Rule>) -> Result<QueryNode, ParseError> {
    match query.as_rule() {
        Rule::string => {
            let x = get_string_inner(query);
            Ok(QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String(x)),
            ))
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
            let lhs = infix.next().unwrap();
            let infix_function = infix.next().unwrap();
            let rhs = infix.next().unwrap();

            let lhs = parse_test_value(lhs);
            let rhs = parse_test_value(rhs);

            let test_type = match infix_function.as_str() {
                "==" => TestOperator::Equal,
                "!=" => TestOperator::NotEqual,
                "<" => TestOperator::LessThan,
                "<=" => TestOperator::LessThanOrEqual,
                ">" => TestOperator::GreaterThan,
                ">=" => TestOperator::GreaterThanOrEqual,
                _ => return Err(unknown_infix_error(infix_function)),
            };

            Ok(QueryNode::Test(test_type, lhs, rhs))
        }
        Rule::singleVariableFunc => {
            let mut func = query.into_inner();
            let func_name = func.next().unwrap().as_str();
            let arg = func.next().unwrap();
            let inner = parse_body(arg.into_inner())?;
            let node = match func_name {
                "latest" => QueryNode::Latest(Some(Box::new(inner))),
                "single" => QueryNode::Single(Box::new(inner)),
                _ => unreachable!(),
            };
            Ok(node)
        }
        Rule::brackets => {
            let expr = query.into_inner();
            let inner = parse_body(expr.peek().unwrap().into_inner())?;
            Ok(QueryNode::Brackets(Box::new(inner)))
        }
        _ => unreachable!(),
    }
}

fn parse_test_value(value: Pair<Rule>) -> TestValue {
    match value.as_rule() {
        Rule::lookup => TestValue::Lookup(parse_lookup(get_first_inner_pair(value))),
        Rule::literal => TestValue::Literal(parse_literal(get_first_inner_pair(value))),
        _ => unreachable!(),
    }
}

fn parse_lookup(lookup: Pair<Rule>) -> Lookup {
    match lookup.as_rule() {
        Rule::lookupPacket => Lookup::Packet(parse_lookup_packet(get_first_inner_pair(lookup))),
        Rule::lookupThis => Lookup::This(get_string_inner(lookup)),
        Rule::lookupEnvironment => Lookup::Environment(get_string_inner(lookup)),
        _ => unreachable!(),
    }
}

fn parse_lookup_packet(lookup: pest::iterators::Pair<Rule>) -> PacketLookup {
    match lookup.as_rule() {
        Rule::lookupPacketId => PacketLookup::Id,
        Rule::lookupPacketName => PacketLookup::Name,
        Rule::lookupPacketParam => PacketLookup::Parameter(get_string_inner(lookup)),
        _ => unreachable!(),
    }
}

fn parse_literal(literal: Pair<Rule>) -> Literal {
    match literal.as_rule() {
        Rule::string => Literal::String(get_string_inner(literal)),
        Rule::boolean => Literal::Bool(literal.as_str().to_lowercase().parse().unwrap()),
        Rule::number => Literal::Number(literal.as_str().parse().unwrap()),
        _ => unreachable!(),
    }
}

fn unknown_infix_error(operator: Pair<Rule>) -> ParseError {
    pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError {
            message: format!("Encountered unknown infix operator: {}", operator.as_str()),
        },
        operator.as_span(),
    )
    .into()
}

fn get_string_inner(rule: Pair<Rule>) -> &str {
    get_first_inner_pair(rule).as_str()
}

fn get_first_inner_pair(rule: Pair<Rule>) -> Pair<Rule> {
    rule.into_inner().peek().unwrap()
}

#[cfg(test)]
mod tests {
    use crate::query::test_utils_query::tests::assert_query_node_lookup_number_eq;

    use super::*;

    macro_rules! assert_node {
        ( $res:expr, $node:pat ) => {
            assert!(matches!($res, $node), "Nodes don't match,\nexpected: {:?}\ngot: {:?}", stringify!($node), $res)
        };
        ( $res:expr, QueryNode::BooleanExpr, $op:path, ($($nested1:tt)*), ($($nested2:tt)*) ) => {
            match $res {
                QueryNode::BooleanExpr($op, value1, value2) => {
                    assert_node!(*value1, $($nested1)*);
                    assert_node!(*value2, $($nested2)*);
                }
                _ => panic!("Invalid type,\nexpected: QueryNode::BooleanExpr({:?}, _, _)\ngot: {:?}", stringify!($op), $res)
            }
        };
        ( $res:expr, QueryNode::Latest, ($($nested:tt)*) ) => {
            match $res {
                QueryNode::Latest(Some(value)) => {
                    assert_node!(*value, $($nested)*);
                }
                _ => panic!("Invalid type,\nexpected: QueryNode::Latest(_)\ngot: {:?}", $res)
            }
        };
        ( $res:expr, $path:path, ($($nested:tt)*) ) => {
            match $res {
                $path(value) => {
                    assert_node!(*value, $($nested)*);
                },
                _ => panic!("Invalid type,\nexpected: {}\ngot: {:?}", stringify!($path), $res),
            };
        };
    }

    #[test]
    fn query_can_parse_shortforms() {
        let res = parse_query("latest").unwrap();
        assert_node!(res, QueryNode::Latest(None));

        let res = parse_query(r#""123""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("123"))
            )
        );

        let res = parse_query(r#"'123'"#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("123"))
            )
        );

        // Shortforms aren't allowed nested in complex expressions.
        assert!(parse_query("latest('123')").is_err());

        // Only string literals are allowed as shortforms. No integer
        // or booleans.
        assert!(parse_query("123").is_err());
        assert!(parse_query("false").is_err());
    }

    #[test]
    fn query_can_be_parsed() {
        let res = parse_query("latest()").unwrap();
        assert_node!(res, QueryNode::Latest(None));
        let res = parse_query(r#"id == "123""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("123"))
            )
        );
        let res = parse_query("id == '123'").unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("123"))
            )
        );
        let res = parse_query(r#"id == "12 3""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("12 3"))
            )
        );
        let res = parse_query(r#"name == "123""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Name)),
                TestValue::Literal(Literal::String("123"))
            )
        );
        let res = parse_query(r#"name == '1"23'"#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Name)),
                TestValue::Literal(Literal::String(r#"1"23"#))
            )
        );
        let res = parse_query(r#"latest(id == "123")"#).unwrap();
        assert_node!(
            res,
            QueryNode::Latest,
            (QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("123"))
            ))
        );
        let res = parse_query(r#"latest(name == "example")"#).unwrap();
        assert_node!(
            res,
            QueryNode::Latest,
            (QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Name)),
                TestValue::Literal(Literal::String("example"))
            ))
        );
    }

    #[test]
    fn query_can_parse_parameters() {
        let res = parse_query(r#"parameter:x == "foo""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
                TestValue::Literal(Literal::String("foo"))
            )
        );
        let res = parse_query(r#"parameter:x=="foo""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
                TestValue::Literal(Literal::String("foo"))
            )
        );
        let res = parse_query(r#"parameter:longer=="foo""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("longer"))),
                TestValue::Literal(Literal::String("foo"))
            )
        );
        let res = parse_query(r#"parameter:x123=="foo""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x123"))),
                TestValue::Literal(Literal::String("foo"))
            )
        );
        let res = parse_query("parameter:x == true").unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
                TestValue::Literal(Literal::Bool(true))
            )
        );
        let res = parse_query("parameter:x == TRUE").unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
                TestValue::Literal(Literal::Bool(true))
            )
        );
        let res = parse_query("parameter:x == True").unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
                TestValue::Literal(Literal::Bool(true))
            )
        );
        let res = parse_query("parameter:x == false").unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
                TestValue::Literal(Literal::Bool(false))
            )
        );
        let res = parse_query("parameter:x == FALSE").unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
                TestValue::Literal(Literal::Bool(false))
            )
        );
        let res = parse_query("parameter:x == False").unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
                TestValue::Literal(Literal::Bool(false))
            )
        );
        let e = parse_query("parameter:x == T").unwrap_err();
        assert!(e.to_string().contains("expected lookup or literal"));

        let res = parse_query("parameter:x == 2").unwrap();
        assert_query_node_lookup_number_eq(
            res,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
            2.0,
        );
        let res = parse_query("parameter:x == +2").unwrap();
        assert_query_node_lookup_number_eq(
            res,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
            2.0,
        );
        let res = parse_query("parameter:x == 2.0").unwrap();
        assert_query_node_lookup_number_eq(
            res,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
            2.0,
        );
        let res = parse_query("parameter:x == 2.").unwrap();
        assert_query_node_lookup_number_eq(
            res,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
            2.0,
        );
        let res = parse_query("parameter:x == -2.0").unwrap();
        assert_query_node_lookup_number_eq(
            res,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
            -2.0,
        );
        let res = parse_query("parameter:x == +2.0").unwrap();
        assert_query_node_lookup_number_eq(
            res,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
            2.0,
        );
        let res = parse_query("parameter:x == 1e3").unwrap();
        assert_query_node_lookup_number_eq(
            res,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
            1000.0,
        );
        let res = parse_query("parameter:x == 1e+3").unwrap();
        assert_query_node_lookup_number_eq(
            res,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
            1000.0,
        );
        let res = parse_query("parameter:x == 2.3e-2").unwrap();
        assert_query_node_lookup_number_eq(
            res,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
            0.023,
        );
        let res = parse_query("parameter:x == -2.3e-2").unwrap();
        assert_query_node_lookup_number_eq(
            res,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
            -0.023,
        );
    }

    #[test]
    fn query_can_parse_tests() {
        let res = parse_query(r#"id == "123""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("123"))
            )
        );
        let res = parse_query(r#"id != "123""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::NotEqual,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("123"))
            )
        );
        let res = parse_query(r#"id < "123""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::LessThan,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("123"))
            )
        );
        let res = parse_query(r#"id <= "123""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::LessThanOrEqual,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("123"))
            )
        );
        let res = parse_query(r#"id > "123""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::GreaterThan,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("123"))
            )
        );
        let res = parse_query(r#"id >= "123""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::GreaterThanOrEqual,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("123"))
            )
        );

        let e = parse_query(r#"name =! "123""#).unwrap_err();
        assert!(e
            .to_string()
            .contains("Encountered unknown infix operator: =!"));
    }

    #[test]
    fn query_can_parse_negation_and_brackets() {
        let res = parse_query("!latest()").unwrap();
        assert_node!(res, QueryNode::Negation, (QueryNode::Latest(None)));

        let res = parse_query("(latest())").unwrap();
        assert_node!(res, QueryNode::Brackets, (QueryNode::Latest(None)));

        let res = parse_query(r#"id == "123""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("123"))
            )
        );

        let res = parse_query(r#"!id == "123""#).unwrap();
        assert_node!(
            res,
            QueryNode::Negation,
            (QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("123"))
            ))
        );

        let res = parse_query(r#"(!id == "123")"#).unwrap();
        assert_node!(
            res,
            QueryNode::Brackets,
            (
                QueryNode::Negation,
                (QueryNode::Test(
                    TestOperator::Equal,
                    TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                    TestValue::Literal(Literal::String("123"))
                ))
            )
        );

        let res = parse_query(r#"!(!id == "123")"#).unwrap();
        assert_node!(
            res,
            QueryNode::Negation,
            (
                QueryNode::Brackets,
                (
                    QueryNode::Negation,
                    (QueryNode::Test(
                        TestOperator::Equal,
                        TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                        TestValue::Literal(Literal::String("123"))
                    ))
                )
            )
        );
    }

    #[test]
    fn query_can_parse_logical_operators() {
        let res = parse_query(r#"id == "123" || id == "345""#).unwrap();
        assert_node!(
            res,
            QueryNode::BooleanExpr,
            BooleanOperator::Or,
            (QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("123"))
            )),
            (QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("345"))
            ))
        );

        let res = parse_query(r#"id == "123" && id == "345""#).unwrap();
        assert_node!(
            res,
            QueryNode::BooleanExpr,
            BooleanOperator::And,
            (QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("123"))
            )),
            (QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("345"))
            ))
        );

        let res = parse_query(r#"id == "123" && id == "345" || id == "this""#).unwrap();
        assert_node!(
            res,
            QueryNode::BooleanExpr,
            BooleanOperator::Or,
            (
                QueryNode::BooleanExpr,
                BooleanOperator::And,
                (QueryNode::Test(
                    TestOperator::Equal,
                    TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                    TestValue::Literal(Literal::String("123"))
                )),
                (QueryNode::Test(
                    TestOperator::Equal,
                    TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                    TestValue::Literal(Literal::String("345"))
                ))
            ),
            (QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("this"))
            ))
        );

        let res = parse_query(r#"id == "this" || id == "123" && id == "345""#).unwrap();
        assert_node!(
            res,
            QueryNode::BooleanExpr,
            BooleanOperator::Or,
            (QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("this"))
            )),
            (
                QueryNode::BooleanExpr,
                BooleanOperator::And,
                (QueryNode::Test(
                    TestOperator::Equal,
                    TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                    TestValue::Literal(Literal::String("123"))
                )),
                (QueryNode::Test(
                    TestOperator::Equal,
                    TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                    TestValue::Literal(Literal::String("345"))
                ))
            )
        );

        let res = parse_query(r#"(id == "this" || id == "123") && id == "345""#).unwrap();
        assert_node!(
            res,
            QueryNode::BooleanExpr,
            BooleanOperator::And,
            (
                QueryNode::Brackets,
                (
                    QueryNode::BooleanExpr,
                    BooleanOperator::Or,
                    (QueryNode::Test(
                        TestOperator::Equal,
                        TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                        TestValue::Literal(Literal::String("this"))
                    )),
                    (QueryNode::Test(
                        TestOperator::Equal,
                        TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                        TestValue::Literal(Literal::String("123"))
                    ))
                )
            ),
            (QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                TestValue::Literal(Literal::String("345"))
            ))
        );
    }

    #[test]
    fn query_can_parse_nested_latest() {
        let res = parse_query(r#"latest(id == "123" || name == "this")"#).unwrap();
        assert_node!(
            res,
            QueryNode::Latest,
            (
                QueryNode::BooleanExpr,
                BooleanOperator::Or,
                (QueryNode::Test(
                    TestOperator::Equal,
                    TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
                    TestValue::Literal(Literal::String("123"))
                )),
                (QueryNode::Test(
                    TestOperator::Equal,
                    TestValue::Lookup(Lookup::Packet(PacketLookup::Name)),
                    TestValue::Literal(Literal::String("this"))
                ))
            )
        );
    }

    #[test]
    fn query_can_parse_single_func() {
        let res = parse_query(r#"single(parameter:x == "foo")"#).unwrap();
        assert_node!(
            res,
            QueryNode::Single,
            (QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
                TestValue::Literal(Literal::String("foo"))
            ))
        );

        let e = parse_query(r#"single()"#).unwrap_err();
        assert!(e.to_string().contains("expected body"));
    }

    #[test]
    fn query_can_parse_infix_in_any_order() {
        let res = parse_query(r#"parameter:x == "foo""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
                TestValue::Literal(Literal::String("foo"))
            )
        );
        let res = parse_query(r#""foo" == parameter:x"#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Literal(Literal::String("foo")),
                TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x")))
            )
        );

        let res = parse_query(r#"parameter:x < "foo""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::LessThan,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
                TestValue::Literal(Literal::String("foo"))
            )
        );
        let res = parse_query(r#""foo" < parameter:x"#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::LessThan,
                TestValue::Literal(Literal::String("foo")),
                TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x")))
            )
        );

        let res = parse_query(r#""foo" == "foo""#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Literal(Literal::String("foo")),
                TestValue::Literal(Literal::String("foo"))
            )
        );

        let res = parse_query(r#"parameter:x == parameter:x"#).unwrap();
        assert_node!(
            res,
            QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
                TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x")))
            )
        );
    }
}
