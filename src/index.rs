use crate::metadata::{get_metadata_from_date, Packet};
use std::io;
use std::path::Path;

#[derive(Clone)]
pub struct Index {
    pub packets: Vec<Packet>,
}

pub fn get_packet_index(root: &Path) -> io::Result<Index> {
    let packets = get_metadata_from_date(root, None)?;
    Ok(Index { packets })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_get_packet_index() {
        let index = get_packet_index(Path::new("tests/example")).unwrap();
        assert_eq!(index.packets.len(), 4);
        let ids: Vec<String> = index
            .packets
            .iter()
            .map(|packet| packet.id.clone())
            .collect();
        assert_eq!(ids[0], "20170818-164830-33e0ab01");
        assert_eq!(ids[1], "20170818-164847-7574883b");
        assert_eq!(ids[2], "20180220-095832-16a4bbed");
        assert_eq!(ids[3], "20180818-164043-7cdcde4b");
    }
}
