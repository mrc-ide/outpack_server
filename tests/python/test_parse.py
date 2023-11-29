import pytest
from outpack_query_parser import parse_query, Latest, Operator, Literal, LookupName, Literal

# Calling this Test makes pytest think it's a test class and freak out
from outpack_query_parser import Test as NodeTest

def test_parse():
    assert parse_query("latest") == Latest(None)
    assert parse_query("latest()") == Latest(None)
    assert parse_query("name == 'foo'") == NodeTest(Operator.Equal, LookupName(), Literal("foo"))

def test_error():
    with pytest.raises(ValueError, match="expected query"):
        parse_query("foo")
