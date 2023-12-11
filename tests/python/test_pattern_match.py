import pytest
from outpack_query_parser import parse_query, Latest, Literal, LookupName, Literal

# Importing Test* types makes pytest freak out. Use a short module name instead.
import outpack_query_parser as parser

def test_pattern_match():
    match parse_query("latest"):
        case Latest(None):
            pass
        case _:
            pytest.fail("pattern did not match")

    match parse_query("latest(name == 'foo')"):
        case Latest(parser.Test()):
            pass
        case _:
            pytest.fail("pattern did not match")

    match parse_query("name == 'foo'"):
        case parser.Test(parser.TestOperator.Equal, LookupName, Literal("foo")):
            pass
        case _:
            pytest.fail("pattern did not match")
