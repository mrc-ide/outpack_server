use crate::metadata::Packet;
use crate::query::QueryError;

pub fn format_query_result(
    packets: Result<Vec<&Packet>, QueryError>,
) -> Result<String, QueryError> {
    let returned_packets = packets?;
    let mut packets_iter = returned_packets.iter().peekable();
    if packets_iter.peek().is_some() {
        Ok(itertools::Itertools::intersperse(
            packets_iter.map(|packet| packet.id.clone()),
            String::from("\n"),
        )
        .collect())
    } else {
        Ok(String::from("Found no packets"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;

    #[test]
    fn query_result_can_be_formatted() {
        let packets =
            crate::metadata::get_metadata_from_date(Path::new("tests/example"), None).unwrap();
        let packet_refs: Vec<&Packet> = packets.iter().collect();
        let one_packet = vec![packet_refs[0]];

        let res = format_query_result(Ok(packet_refs)).unwrap();
        assert_eq!(
            res,
            "20170818-164830-33e0ab01\n20170818-164847-7574883b\n20180220-095832-16a4bbed\n20180818-164043-7cdcde4b"
        );

        let res = format_query_result(Ok(one_packet)).unwrap();
        assert_eq!(res, "20170818-164830-33e0ab01");

        let res = format_query_result(Ok(vec![])).unwrap();
        assert_eq!(res, "Found no packets")
    }

    #[test]
    fn query_format_propagates_error() {
        let e =
            format_query_result(Err(QueryError::EvalError(String::from("An error")))).unwrap_err();
        assert!(matches!(e, QueryError::EvalError(..)));
        assert!(e.to_string().contains("An error"));
    }
}
