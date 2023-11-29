use std::collections::HashSet;

use serde_json::value::Value as JsonValue;

use crate::index::Index;
use crate::metadata::Packet;
use crate::query::query_types::*;
use crate::query::QueryError;

pub fn eval_query<'a>(index: &'a Index, query: QueryNode) -> Result<Vec<&'a Packet>, QueryError> {
    match query {
        QueryNode::Latest(inner) => eval_latest(index, inner),
        QueryNode::Single(inner) => eval_single(index, *inner),
        QueryNode::Test(test, lhs, rhs) => eval_test(index, test, lhs, rhs),
        QueryNode::Negation(inner) => eval_negation(index, *inner),
        QueryNode::Brackets(inner) => eval_brackets(index, *inner),
        QueryNode::BooleanExpr(op, lhs, rhs) => eval_boolean_op(index, op, *lhs, *rhs),
    }
}

fn eval_latest<'a>(
    index: &'a Index,
    inner: Option<Box<QueryNode>>,
) -> Result<Vec<&'a Packet>, QueryError> {
    if inner.is_some() {
        let latest = eval_query(index, *inner.unwrap())?;
        let last = latest.last();
        match last {
            Some(packet) => Ok(vec![*packet]),
            None => Ok(vec![]),
        }
    } else {
        let last = index.packets.last();
        match last {
            Some(packet) => Ok(vec![packet]),
            None => Ok(vec![]),
        }
    }
}

fn eval_single<'a>(index: &'a Index, inner: QueryNode) -> Result<Vec<&'a Packet>, QueryError> {
    let packets = eval_query(index, inner)?;
    if packets.len() != 1 {
        Err(QueryError::EvalError(format!(
            "Query found {} packets, but expected exactly one",
            packets.len()
        )))
    } else {
        Ok(packets)
    }
}

fn eval_negation<'a>(index: &'a Index, inner: QueryNode) -> Result<Vec<&'a Packet>, QueryError> {
    let packets = eval_query(index, inner)?;
    Ok(index
        .packets
        .iter()
        .filter(|packet| !packets.contains(packet))
        .collect())
}

fn eval_brackets<'a>(index: &'a Index, inner: QueryNode) -> Result<Vec<&'a Packet>, QueryError> {
    eval_query(index, inner)
}

fn eval_test<'a>(
    index: &'a Index,
    test: TestOperator,
    lhs: TestValue,
    rhs: TestValue,
) -> Result<Vec<&'a Packet>, QueryError> {
    index
        .packets
        .iter()
        .filter_map(|packet| match lookup_filter(packet, &test, &lhs, &rhs) {
            Ok(true) => Some(Ok(packet)),
            Ok(false) => None,
            Err(err) => Some(Err(err)),
        })
        .collect()
}

fn lookup_filter(
    packet: &Packet,
    test: &TestOperator,
    lhs: &TestValue,
    rhs: &TestValue,
) -> Result<bool, QueryError> {
    let lhs_literal = evaluate_test_value(packet, lhs)?;
    let rhs_literal = evaluate_test_value(packet, rhs)?;

    Ok(match (test, lhs_literal, rhs_literal) {
        (test, Some(Literal::Number(l)), Some(Literal::Number(r))) => match test {
            TestOperator::Equal => l == r,
            TestOperator::NotEqual => l != r,
            TestOperator::LessThan => l < r,
            TestOperator::LessThanOrEqual => l <= r,
            TestOperator::GreaterThan => l > r,
            TestOperator::GreaterThanOrEqual => l >= r,
        },
        (TestOperator::Equal, Some(l), Some(r)) => l == r,
        (TestOperator::NotEqual, Some(l), Some(r)) => l != r,
        (_, _, _) => false,
    })
}

fn evaluate_test_value<'a>(
    packet: &'a Packet,
    value: &'a TestValue,
) -> Result<Option<Literal<'a>>, QueryError> {
    match value {
        TestValue::Literal(value) => Ok(Some(value.clone())),
        TestValue::Lookup(lookup) => evaluate_lookup(packet, lookup),
    }
}

fn evaluate_lookup<'a>(
    packet: &'a Packet,
    lookup: &'a Lookup,
) -> Result<Option<Literal<'a>>, QueryError> {
    match lookup {
        Lookup::Packet(lookup) => Ok(packet.lookup_value(lookup)),
        Lookup::Environment(_) => Err(QueryError::EvalError(
            "environment lookup is not supported in this context".into(),
        )),
        Lookup::This(_) => Err(QueryError::EvalError(
            "this parameters are not supported in this context".into(),
        )),
    }
}

impl Packet {
    pub fn lookup_value(&self, lookup: &PacketLookup) -> Option<Literal> {
        match lookup {
            PacketLookup::Id => Some(Literal::String(&self.id)),
            PacketLookup::Name => Some(Literal::String(&self.name)),
            PacketLookup::Parameter(param_name) => self.get_parameter(param_name),
        }
    }

    pub fn get_parameter(&self, param_name: &str) -> Option<Literal> {
        if let Some(params) = &self.parameters {
            match params.get(param_name)? {
                JsonValue::Number(number) => Some(Literal::Number(number.as_f64()?)),
                JsonValue::Bool(bool) => Some(Literal::Bool(*bool)),
                JsonValue::String(string) => Some(Literal::String(string)),
                _ => None, // Parameters must be number, bool or string
            }
        } else {
            None
        }
    }
}

fn eval_boolean_op<'a>(
    index: &'a Index,
    op: BooleanOperator,
    lhs: QueryNode,
    rhs: QueryNode,
) -> Result<Vec<&'a Packet>, QueryError> {
    let lhs_res = eval_query(index, lhs)?;
    let rhs_res = eval_query(index, rhs)?;
    let lhs_set: HashSet<&Packet> = HashSet::from_iter(lhs_res.iter().cloned());
    let rhs_set: HashSet<&Packet> = HashSet::from_iter(rhs_res.iter().cloned());
    match op {
        BooleanOperator::And => Ok(lhs_set.intersection(&rhs_set).copied().collect()),
        BooleanOperator::Or => Ok(lhs_set.union(&rhs_set).copied().collect()),
    }
}

#[cfg(test)]
mod tests {
    use crate::metadata::get_metadata_from_date;
    use crate::test_utils::tests::assert_packet_ids_eq;

    use super::*;

    #[test]
    fn query_lookup_works() {
        let index = crate::index::get_packet_index("tests/example").unwrap();

        let query = QueryNode::Test(
            TestOperator::Equal,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
            TestValue::Literal(Literal::String("20180818-164043-7cdcde4b")),
        );
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(res, vec!["20180818-164043-7cdcde4b"]);

        let query = QueryNode::Test(
            TestOperator::Equal,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Name)),
            TestValue::Literal(Literal::String("modup-201707-queries1")),
        );
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(
            res,
            vec![
                "20170818-164830-33e0ab01",
                "20170818-164847-7574883b",
                "20180818-164043-7cdcde4b",
            ],
        );

        let query = QueryNode::Test(
            TestOperator::Equal,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Id)),
            TestValue::Literal(Literal::String("123")),
        );
        let res = eval_query(&index, query).unwrap();
        assert_eq!(res.len(), 0);

        let query = QueryNode::Test(
            TestOperator::Equal,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("disease"))),
            TestValue::Literal(Literal::String("YF")),
        );
        let res = eval_query(&index, query).unwrap();
        assert_eq!(res.len(), 3);

        let query = QueryNode::Test(
            TestOperator::Equal,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("foo"))),
            TestValue::Literal(Literal::String("bar")),
        );
        let res = eval_query(&index, query).unwrap();
        assert_eq!(res.len(), 0);
    }

    #[test]
    fn query_latest_works() {
        let index = crate::index::get_packet_index("tests/example").unwrap();

        let query = QueryNode::Latest(None);
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(res, vec!["20180818-164043-7cdcde4b"]);

        let inner_query = QueryNode::Test(
            TestOperator::Equal,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Name)),
            TestValue::Literal(Literal::String("modup-201707-queries1")),
        );
        let query = QueryNode::Latest(Some(Box::new(inner_query)));
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(res, vec!["20180818-164043-7cdcde4b"]);

        let inner_query = QueryNode::Test(
            TestOperator::Equal,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Name)),
            TestValue::Literal(Literal::String("123")),
        );
        let query = QueryNode::Latest(Some(Box::new(inner_query)));
        let res = eval_query(&index, query).unwrap();
        assert_eq!(res.len(), 0);
    }

    #[test]
    fn can_get_parameter_as_literal() {
        let packets = get_metadata_from_date("tests/example", None).unwrap();
        assert_eq!(packets.len(), 4);

        let matching_packets: Vec<Packet> = packets
            .into_iter()
            .filter(|e| e.id == "20180220-095832-16a4bbed")
            .collect();
        assert_eq!(matching_packets.len(), 1);

        let packet = matching_packets.first().unwrap();
        assert_eq!(packet.id, "20180220-095832-16a4bbed");

        assert_eq!(packet.get_parameter("missing"), None);
        assert_eq!(packet.get_parameter("disease"), Some(Literal::String("YF")));
        assert_eq!(packet.get_parameter("pull_data"), Some(Literal::Bool(true)));
        assert_eq!(
            packet.get_parameter("tolerance"),
            Some(Literal::Number(0.001))
        );
        assert_eq!(packet.get_parameter("size"), Some(Literal::Number(10f64)));
    }

    #[test]
    fn can_test_lookup_filter() {
        let packets = get_metadata_from_date("tests/example", None).unwrap();
        assert_eq!(packets.len(), 4);

        let matching_packets: Vec<Packet> = packets
            .into_iter()
            .filter(|e| e.id == "20180220-095832-16a4bbed")
            .collect();
        assert_eq!(matching_packets.len(), 1);

        let packet = matching_packets.first().unwrap();
        assert_eq!(packet.id, "20180220-095832-16a4bbed");
        assert_eq!(packet.name, "modup-201707-params1");
        assert!(packet.parameters.is_some());

        let params = packet.parameters.clone().unwrap();
        assert_eq!(params.len(), 4);
        assert_eq!(
            params.get("tolerance").unwrap(),
            &(serde_json::Value::Number(serde_json::Number::from_f64(0.001).unwrap()))
        );
        assert_eq!(
            params.get("size").unwrap(),
            &(serde_json::Value::Number(serde_json::Number::from(10)))
        );
        assert_eq!(
            params.get("disease").unwrap(),
            &(serde_json::Value::String(String::from("YF")))
        );
        assert_eq!(
            params.get("pull_data").unwrap(),
            &(serde_json::Value::Bool(true))
        );

        macro_rules! test_param {
            ( $( $test:expr, $lhs:expr, $rhs:expr => $result:literal )* ) => {
                $(
                if $result {
                    assert!(lookup_filter(&packet, $test, $lhs, $rhs).unwrap());
                } else {
                    assert!(!lookup_filter(&packet, $test, $lhs, $rhs).unwrap());
                }
                )*
            };
        }

        test_param!(
            &TestOperator::Equal, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance"))), &TestValue::Literal(Literal::Number(0.001))   => true
            &TestOperator::Equal, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance"))), &TestValue::Literal(Literal::Number(0.002))   => false
            &TestOperator::Equal, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance"))), &TestValue::Literal(Literal::String("0.001")) => false

            &TestOperator::Equal, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("disease"))), &TestValue::Literal(Literal::String("YF"))   => true
            &TestOperator::Equal, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("disease"))), &TestValue::Literal(Literal::String("HepB")) => false
            &TestOperator::Equal, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("disease"))), &TestValue::Literal(Literal::Number(0.5))    => false

            &TestOperator::Equal, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("size"))), &TestValue::Literal(Literal::Number(10f64)) => true
            &TestOperator::Equal, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("size"))), &TestValue::Literal(Literal::Number(10.0))  => true
            &TestOperator::Equal, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("size"))), &TestValue::Literal(Literal::Number(9f64))  => false
            &TestOperator::Equal, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("size"))), &TestValue::Literal(Literal::Bool(true))    => false

            &TestOperator::Equal, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("pull_data"))), &TestValue::Literal(Literal::Bool(true))     => true
            &TestOperator::Equal, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("pull_data"))), &TestValue::Literal(Literal::Bool(false))    => false
            &TestOperator::Equal, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("pull_data"))), &TestValue::Literal(Literal::String("true")) => false

            &TestOperator::NotEqual,           &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance"))), &TestValue::Literal(Literal::Number(0.002)) => true
            &TestOperator::LessThan,           &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance"))), &TestValue::Literal(Literal::Number(0.002)) => true
            &TestOperator::LessThanOrEqual,    &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance"))), &TestValue::Literal(Literal::Number(0.002)) => true
            &TestOperator::GreaterThan,        &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance"))), &TestValue::Literal(Literal::Number(0.000)) => true
            &TestOperator::GreaterThanOrEqual, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance"))), &TestValue::Literal(Literal::Number(0.000)) => true
            &TestOperator::LessThan,           &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance"))), &TestValue::Literal(Literal::Number(0.000)) => false
            &TestOperator::LessThanOrEqual,    &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance"))), &TestValue::Literal(Literal::Number(0.000)) => false

            &TestOperator::LessThan, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("pull_data"))), &TestValue::Literal(Literal::Bool(true))  => false
            &TestOperator::LessThan, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("pull_data"))), &TestValue::Literal(Literal::Bool(false)) => false

            &TestOperator::LessThan,           &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("disease"))), &TestValue::Literal(Literal::String("YF")) => false
            &TestOperator::LessThanOrEqual,    &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("disease"))), &TestValue::Literal(Literal::String("YF")) => false
            &TestOperator::GreaterThan,        &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("disease"))), &TestValue::Literal(Literal::String("YF")) => false
            &TestOperator::GreaterThanOrEqual, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("disease"))), &TestValue::Literal(Literal::String("YF")) => false

            &TestOperator::Equal, &TestValue::Literal(Literal::Number(0.001)), &TestValue::Literal(Literal::Number(0.001))    => true
            &TestOperator::Equal, &TestValue::Literal(Literal::Number(0.001)), &TestValue::Literal(Literal::Number(0.002))    => false
            &TestOperator::Equal, &TestValue::Literal(Literal::Number(0.001)), &TestValue::Literal(Literal::String("0.002"))  => false
            &TestOperator::NotEqual, &TestValue::Literal(Literal::Number(0.001)), &TestValue::Literal(Literal::Number(0.001)) => false

            &TestOperator::Equal, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance"))), &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance")))           => true
            &TestOperator::NotEqual, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance"))), &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance")))        => false
            &TestOperator::GreaterThan, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance"))), &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance")))     => false
            &TestOperator::LessThanOrEqual, &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance"))), &TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("tolerance"))) => true
        );
    }

    #[test]
    fn can_use_different_test_types() {
        let index = crate::index::get_packet_index("tests/example").unwrap();

        let query = QueryNode::Test(
            TestOperator::Equal,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Name)),
            TestValue::Literal(Literal::String("modup-201707-params1")),
        );
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(res, vec!["20180220-095832-16a4bbed"]);
        let query = QueryNode::Test(
            TestOperator::Equal,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("size"))),
            TestValue::Literal(Literal::Number(10f64)),
        );
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(res, vec!["20180220-095832-16a4bbed"]);

        let query = QueryNode::Test(
            TestOperator::LessThan,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("size"))),
            TestValue::Literal(Literal::Number(10.1f64)),
        );
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(res, vec!["20180220-095832-16a4bbed"]);
        let query = QueryNode::Test(
            TestOperator::GreaterThan,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("size"))),
            TestValue::Literal(Literal::Number(9.4f64)),
        );
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(res, vec!["20180220-095832-16a4bbed"]);
        let query = QueryNode::Test(
            TestOperator::GreaterThan,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("size"))),
            TestValue::Literal(Literal::Number(10f64)),
        );
        let res = eval_query(&index, query).unwrap();
        assert_eq!(res.len(), 0);
        let query = QueryNode::Test(
            TestOperator::GreaterThanOrEqual,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("size"))),
            TestValue::Literal(Literal::Number(10f64)),
        );
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(res, vec!["20180220-095832-16a4bbed"]);
        let query = QueryNode::Test(
            TestOperator::LessThanOrEqual,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("size"))),
            TestValue::Literal(Literal::Number(10f64)),
        );
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(res, vec!["20180220-095832-16a4bbed"]);

        let query = QueryNode::Test(
            TestOperator::NotEqual,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("pull_data"))),
            TestValue::Literal(Literal::Bool(false)),
        );
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(res, vec!["20180220-095832-16a4bbed"]);
        let query = QueryNode::Test(
            TestOperator::NotEqual,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("pull_data"))),
            TestValue::Literal(Literal::Bool(true)),
        );
        let res = eval_query(&index, query).unwrap();
        assert_eq!(res.len(), 0);
    }

    #[test]
    fn invalid_comparisons_dont_match() {
        let index = crate::index::get_packet_index("tests/example").unwrap();

        let query = QueryNode::Test(
            TestOperator::GreaterThan,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("disease"))),
            TestValue::Literal(Literal::String("ABC")),
        );
        let res = eval_query(&index, query).unwrap();
        assert_eq!(res.len(), 0);
        let query = QueryNode::Test(
            TestOperator::LessThan,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("disease"))),
            TestValue::Literal(Literal::String("ABC")),
        );
        let res = eval_query(&index, query).unwrap();
        assert_eq!(res.len(), 0);
        let query = QueryNode::Test(
            TestOperator::GreaterThanOrEqual,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("disease"))),
            TestValue::Literal(Literal::String("YF")),
        );
        let res = eval_query(&index, query).unwrap();
        assert_eq!(res.len(), 0);
        let query = QueryNode::Test(
            TestOperator::LessThanOrEqual,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("disease"))),
            TestValue::Literal(Literal::String("YF")),
        );
        let res = eval_query(&index, query).unwrap();
        assert_eq!(res.len(), 0);

        let query = QueryNode::Test(
            TestOperator::GreaterThanOrEqual,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("pull_data"))),
            TestValue::Literal(Literal::Bool(true)),
        );
        let res = eval_query(&index, query).unwrap();
        assert_eq!(res.len(), 0);
        let query = QueryNode::Test(
            TestOperator::LessThanOrEqual,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("pull_data"))),
            TestValue::Literal(Literal::Bool(false)),
        );
        let res = eval_query(&index, query).unwrap();
        assert_eq!(res.len(), 0);
    }

    #[test]
    fn query_does_no_type_coersion() {
        let index = crate::index::get_packet_index("tests/example").unwrap();

        let query = QueryNode::Test(
            TestOperator::Equal,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("pull_data"))),
            TestValue::Literal(Literal::String("TRUE")),
        );
        let res = eval_query(&index, query).unwrap();
        assert_eq!(res.len(), 0);
        let query = QueryNode::Test(
            TestOperator::Equal,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("pull_data"))),
            TestValue::Literal(Literal::String("true")),
        );
        let res = eval_query(&index, query).unwrap();
        assert_eq!(res.len(), 0);
        let query = QueryNode::Test(
            TestOperator::Equal,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("pull_data"))),
            TestValue::Literal(Literal::String("T")),
        );
        let res = eval_query(&index, query).unwrap();
        assert_eq!(res.len(), 0);
        let query = QueryNode::Test(
            TestOperator::Equal,
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("pull_data"))),
            TestValue::Literal(Literal::Number(1f64)),
        );
        let res = eval_query(&index, query).unwrap();
        assert_eq!(res.len(), 0);
    }

    #[test]
    fn query_with_negation_works() {
        let index = crate::index::get_packet_index("tests/example").unwrap();

        let query = QueryNode::Negation(Box::new(QueryNode::Latest(None)));
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(
            res,
            vec![
                "20170818-164830-33e0ab01",
                "20170818-164847-7574883b",
                "20180220-095832-16a4bbed",
            ],
        );

        let query = QueryNode::Negation(Box::new(QueryNode::Negation(Box::new(
            QueryNode::Latest(None),
        ))));
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(res, vec!["20180818-164043-7cdcde4b"]);
    }

    #[test]
    fn query_with_brackets_works() {
        let index = crate::index::get_packet_index("tests/example").unwrap();

        let query = QueryNode::Brackets(Box::new(QueryNode::Latest(None)));
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(res, vec!["20180818-164043-7cdcde4b"]);

        let query = QueryNode::Brackets(Box::new(QueryNode::Brackets(Box::new(
            QueryNode::Latest(None),
        ))));
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(res, vec!["20180818-164043-7cdcde4b"]);

        let query = QueryNode::Brackets(Box::new(QueryNode::Negation(Box::new(
            QueryNode::Latest(None),
        ))));
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(
            res,
            vec![
                "20170818-164830-33e0ab01",
                "20170818-164847-7574883b",
                "20180220-095832-16a4bbed",
            ],
        );
    }

    #[test]
    fn query_with_boolean_operators_works() {
        let index = crate::index::get_packet_index("tests/example").unwrap();

        let query = QueryNode::BooleanExpr(
            BooleanOperator::Or,
            Box::new(QueryNode::Latest(None)),
            Box::new(QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Name)),
                TestValue::Literal(Literal::String("modup-201707-params1")),
            )),
        );
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(
            res,
            vec!["20180818-164043-7cdcde4b", "20180220-095832-16a4bbed"],
        );

        let query = QueryNode::BooleanExpr(
            BooleanOperator::And,
            Box::new(QueryNode::Negation(Box::new(QueryNode::Latest(None)))),
            Box::new(QueryNode::Test(
                TestOperator::Equal,
                TestValue::Lookup(Lookup::Packet(PacketLookup::Name)),
                TestValue::Literal(Literal::String("modup-201707-params1")),
            )),
        );
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(res, vec!["20180220-095832-16a4bbed"]);
    }

    #[test]
    fn query_with_single_works() {
        let index = crate::index::get_packet_index("tests/example").unwrap();

        let query = QueryNode::Single(Box::new(QueryNode::Latest(None)));
        let res = eval_query(&index, query).unwrap();
        assert_packet_ids_eq(res, vec!["20180818-164043-7cdcde4b"]);

        let query = QueryNode::Single(Box::new(QueryNode::Negation(Box::new(QueryNode::Latest(
            None,
        )))));
        let e = eval_query(&index, query).unwrap_err();
        assert!(matches!(e, QueryError::EvalError(..)));
        assert!(e
            .to_string()
            .contains("Query found 3 packets, but expected exactly one"));
    }

    #[test]
    fn query_with_this_fails() {
        let index = crate::index::get_packet_index("tests/example").unwrap();
        let query = QueryNode::Test(
            TestOperator::Equal,
            TestValue::Lookup(Lookup::This("x")),
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
        );

        let e = eval_query(&index, query).unwrap_err();
        assert!(matches!(e, QueryError::EvalError(..)));
    }

    #[test]
    fn query_with_environment_fails() {
        let index = crate::index::get_packet_index("tests/example").unwrap();
        let query = QueryNode::Test(
            TestOperator::Equal,
            TestValue::Lookup(Lookup::Environment("x")),
            TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter("x"))),
        );

        let e = eval_query(&index, query).unwrap_err();
        assert!(matches!(e, QueryError::EvalError(..)));
    }
}
