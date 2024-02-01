use cached::instant::SystemTime;
use lazy_static::lazy_static;
use regex::Regex;
use std::ffi::OsString;
use std::time::UNIX_EPOCH;

lazy_static! {
    static ref ID_REG: Regex = Regex::new(r"^([0-9]{8}-[0-9]{6}-[[:xdigit:]]{8})$").unwrap();
}

pub fn is_packet(name: &OsString) -> bool {
    let o = name.to_str();
    o.map_or(false, is_packet_str)
}

pub fn is_packet_str(name: &str) -> bool {
    ID_REG.is_match(name)
}

pub fn time_as_num(time: SystemTime) -> f64 {
    (time.duration_since(UNIX_EPOCH).unwrap().as_millis() as f64) / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn can_detect_packet_id() {
        assert!(!is_packet(&OsString::from("1234")));
        assert!(is_packet(&OsString::from("20170818-164830-33e0ab01")));
        assert!(is_packet(&OsString::from("20180818-164847-54699abf")))
    }

    #[test]
    fn converts_time_to_seconds() {
        let epoch_ms = 1688033668123;
        let time = UNIX_EPOCH + Duration::from_millis(epoch_ms);
        let res = time_as_num(time);
        assert_eq!(res, 1688033668.123);
    }
}
