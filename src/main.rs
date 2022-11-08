extern crate getopts;
use getopts::Options;
use std::env;

fn print_usage(program: &str, opts: getopts::Options) {
    let brief = format!("Usage: {} [options]", program);
    print!("{}", opts.usage(&brief));
}

fn parse_args(args: &[String]) -> Option<String> {

    let program = args[0].clone();

    let mut opts = Options::new();
    opts.reqopt("r", "root", "outpack root path (required)", ".");
    let matches = match opts.parse(&args[1..]) {
        Ok(m) => { m }
        Err(f) => {
            print_usage(&program, opts);
            panic!("{}", f.to_string())
        }
    };
    return Some(matches.opt_str("r").unwrap());
}

#[rocket::main]
#[allow(unused_must_use)]
async fn main() {
    let args = env::args().collect::<Vec<_>>();
    let root = parse_args(&args);
    if root.is_some() {
        outpack_server::api(root.unwrap()).launch().await;
    } else {
        return;
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_parse_args() {
        let root = parse_args(&[String::from("program"), String::from("--root"), String::from("test")]).unwrap();
        assert_eq!(root, "test");

        let root = parse_args(&[String::from("program"), String::from("-r"), String::from("test")]).unwrap();
        assert_eq!(root, "test");
    }

    #[test]
    #[should_panic]
    fn panics_if_args_not_valid() {
        parse_args(&[String::from("program")]);
    }
}
