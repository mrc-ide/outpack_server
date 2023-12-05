use outpack::query::QueryError;

pub fn test_query(root: &str, query: &str, result: &str) {
    let packets = outpack::query::run_query(root, query).unwrap();
    assert_eq!(packets, result);
}

#[test]
fn locates_latest_packet() {
    let root_path = "tests/example";
    test_query(root_path, "latest", "20180818-164043-7cdcde4b");
}

#[test]
fn returns_parse_error_if_syntax_invalid() {
    let root_path = "tests/example";
    let e = outpack::query::run_query(root_path, "invalid").unwrap_err();
    assert!(matches!(e, outpack::query::QueryError::ParseError(..)));
    let text = format!("{}", e);
    assert!(text.contains("Failed to parse query\n"));
}

#[test]
fn eval_error_can_be_displayed() {
    let err = QueryError::EvalError("my error msg".to_string());
    let text = format!("{}", err);
    assert_eq!(text, "Failed to evaluate query\nmy error msg");
}

#[test]
fn can_get_packet_by_id() {
    let root_path = "tests/example";
    test_query(
        root_path,
        r#"id == "20170818-164847-7574883b""#,
        "20170818-164847-7574883b",
    );
    test_query(
        root_path,
        r#"id == "20170818-164830-33e0ab01""#,
        "20170818-164830-33e0ab01",
    );
    test_query(root_path, r#""123""#, "Found no packets");
}

#[test]
fn can_get_packet_by_name() {
    let root_path = "tests/example";
    test_query(
        root_path,
        r#"name == "modup-201707-queries1""#,
        "20170818-164830-33e0ab01\n20170818-164847-7574883b\n20180818-164043-7cdcde4b",
    );
    test_query(
        root_path,
        r#"name == 'modup-201707-queries1'"#,
        "20170818-164830-33e0ab01\n20170818-164847-7574883b\n20180818-164043-7cdcde4b",
    );
    test_query(root_path, r#"name == "notathing""#, "Found no packets");
    let e = outpack::query::run_query(root_path, "name == invalid").unwrap_err();
    assert!(matches!(e, QueryError::ParseError(..)));
    assert!(e.to_string().contains("expected lookup or literal"));
}

#[test]
fn can_get_latest_of_lookup() {
    let root_path = "tests/example";
    test_query(
        root_path,
        r#"latest(name == "modup-201707-queries1")"#,
        "20180818-164043-7cdcde4b",
    );
}

#[test]
fn can_get_packet_by_parameter() {
    let root_path = "tests/example";
    let packets = outpack::query::run_query(root_path, r#"parameter:disease == "YF""#).unwrap();
    assert_eq!(
        packets,
        "20170818-164830-33e0ab01\n20180220-095832-16a4bbed\n\
    20180818-164043-7cdcde4b"
    );
    test_query(
        root_path,
        r#"latest(parameter:disease == "YF")"#,
        "20180818-164043-7cdcde4b",
    );
    test_query(
        root_path,
        r#"latest(parameter:unknown == "YF")"#,
        "Found no packets",
    );
}

#[test]
fn can_get_packet_by_boolean_parameter() {
    let root_path = "tests/example";
    test_query(
        root_path,
        "parameter:pull_data == TRUE",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "parameter:pull_data == true",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "parameter:pull_data == false",
        "Found no packets",
    );
    test_query(
        root_path,
        r#"parameter:pull_data == "true""#,
        "Found no packets",
    );
    test_query(root_path, "parameter:pull_data == 1", "Found no packets");
    test_query(root_path, "parameter:pull_data == 0", "Found no packets");
    let e = outpack::query::run_query(root_path, "parameter:pull_data == T").unwrap_err();
    assert!(matches!(e, QueryError::ParseError(..)));
    assert!(e.to_string().contains("expected lookup or literal"));
}

#[test]
fn can_get_packet_by_numeric_parameter() {
    let root_path = "tests/example";
    test_query(
        root_path,
        "parameter:tolerance == 0.001",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "parameter:tolerance == 1e-3",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "parameter:tolerance == 0.1e-2",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "parameter:tolerance == 0.002",
        "Found no packets",
    );
    test_query(
        root_path,
        "parameter:size == 10",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "parameter:size == 10.0",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "parameter:size == 1e1",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "parameter:size == 1e+1",
        "20180220-095832-16a4bbed",
    );
    test_query(root_path, r#"parameter:size == "10""#, "Found no packets");
}

#[test]
fn no_packets_returned_incompatible_types() {
    let root_path = "tests/example";
    test_query(root_path, "id == 12345", "Found no packets");
    test_query(root_path, "id == true", "Found no packets");
    test_query(root_path, "name == true", "Found no packets");
}

#[test]
fn can_get_packet_other_comparisons() {
    let root_path = "tests/example";
    test_query(
        root_path,
        "parameter:tolerance < 0.002",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "parameter:tolerance <= 0.002",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "parameter:tolerance > 0.1e-5",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "parameter:tolerance >= 0.1e-2",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "parameter:tolerance < 0.1e-2",
        "Found no packets",
    );

    test_query(root_path, r#"parameter:disease < "AB""#, "Found no packets");
    test_query(root_path, r#"parameter:disease > "AB""#, "Found no packets");
    test_query(
        root_path,
        r#"parameter:disease <= "YF""#,
        "Found no packets",
    );
}

#[test]
fn query_supports_groupings() {
    let root_path = "tests/example";
    test_query(
        root_path,
        r#"(name == "modup-201707-queries1")"#,
        "20170818-164830-33e0ab01\n20170818-164847-7574883b\n20180818-164043-7cdcde4b",
    );
    test_query(
        root_path,
        r#"(((name == "modup-201707-queries1")))"#,
        "20170818-164830-33e0ab01\n20170818-164847-7574883b\n20180818-164043-7cdcde4b",
    );
    test_query(
        root_path,
        r#"!(name == "modup-201707-queries1")"#,
        "20180220-095832-16a4bbed",
    );

    test_query(
        root_path,
        r#"(parameter:tolerance < 0.002) || id == "20170818-164847-7574883b""#,
        "20180220-095832-16a4bbed\n20170818-164847-7574883b",
    );
    test_query(
        root_path,
        r#"(parameter:tolerance < 0.002)||id == "20170818-164847-7574883b""#,
        "20180220-095832-16a4bbed\n20170818-164847-7574883b",
    );
    test_query(
        root_path,
        r#"(parameter:tolerance < 0.002) || (id == "20170818-164847-7574883b")"#,
        "20180220-095832-16a4bbed\n20170818-164847-7574883b",
    );

    test_query(
        root_path,
        r#"(parameter:tolerance < 0.002) && id == "20170818-164847-7574883b""#,
        "Found no packets",
    );
    test_query(
        root_path,
        r#"(parameter:tolerance < 0.002) && (id == "20180220-095832-16a4bbed")"#,
        "20180220-095832-16a4bbed",
    );
}

#[test]
fn query_and_is_highest_precedence() {
    // If we have an expression like A || B && C
    // R evaluates this as A || (B && C) so make sure we do this here too
    // i.e. && has higher precedence
    // This difference is clear if A is true, B is true and C is false
    // A || (B && C) -> TRUE
    // (A || B) && C -> FALSE
    let root_path = "tests/example";

    test_query(
        root_path,
        r#"id == "20170818-164847-7574883b" || parameter:tolerance < 0.002 && parameter:size == 10"#,
        "20170818-164847-7574883b\n20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        r#"(id == "20170818-164847-7574883b" || parameter:tolerance < 0.002) && parameter:size == 10"#,
        "20180220-095832-16a4bbed",
    );
}

#[test]
fn query_functions_can_be_nested() {
    let root_path = "tests/example";

    test_query(
        root_path,
        r#"latest(id == "20170818-164847-7574883b" || id == "20180220-095832-16a4bbed")"#,
        "20180220-095832-16a4bbed",
    );
}

#[test]
fn query_can_assert_single_return() {
    let root_path = "tests/example";
    test_query(
        root_path,
        "single(parameter:pull_data == TRUE)",
        "20180220-095832-16a4bbed",
    );
    let e =
        outpack::query::run_query(root_path, "single(parameter:pull_data == false)").unwrap_err();
    assert!(matches!(e, QueryError::EvalError(..)));
    assert!(e
        .to_string()
        .contains("Query found 0 packets, but expected exactly one"));
}

#[test]
fn comparisons_work_in_any_order_with_any_types() {
    let root_path = "tests/example";
    test_query(
        root_path,
        "parameter:pull_data == TRUE",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "TRUE == parameter:pull_data",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "parameter:tolerance < 0.002",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "0.002 > parameter:tolerance",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "parameter:pull_data == parameter:pull_data",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "parameter:tolerance < parameter:size",
        "20180220-095832-16a4bbed",
    );
    test_query(
        root_path,
        "parameter:missing < parameter:size",
        "Found no packets",
    );
    test_query(root_path, "2 == 1", "Found no packets");
    test_query(root_path, "2 != 1",
               "20170818-164830-33e0ab01\n20170818-164847-7574883b\n20180220-095832-16a4bbed\n20180818-164043-7cdcde4b");
}
