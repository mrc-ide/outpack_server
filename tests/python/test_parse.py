import pytest
from outpack_query_parser import parse_query, Latest, Literal, LookupName, Literal

# Importing Test* types makes pytest freak out. Use a short module name instead.
import outpack_query_parser as parser

def test_parse():
    assert parse_query("latest") == Latest(None)
    assert parse_query("latest()") == Latest(None)
    assert parse_query("name == 'foo'") == parser.Test(parser.TestOperator.Equal, LookupName(), Literal("foo"))

def test_error():
    with pytest.raises(ValueError, match="expected query"):
        parse_query("foo")
