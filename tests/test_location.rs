use outpack_server::location;

#[test]
fn can_read_location_entries() {
    let entries = location::read_locations("tests/example")
        .expect("Could not read locations");

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].packet, "20170818-164830-33e0ab01");
    assert_eq!(entries[1].time, 1662480555.6623);
    assert_eq!(entries[1].hash,
               "sha256:5380b3c9a1f93ab3aeaf1ed6367b98aba73dc6bfae3f68fe7d9fe05f57479cbf");

    assert_eq!(entries[0].packet, "20170818-164043-7cdcde4b");
}
