extern crate core;

use getopts::Options;
use std::env;

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options]", program);
    print!("{}", opts.usage(&brief));
}

fn parse_args(args: &[String]) -> (Option<String>, Option<String>) {
    let program = args[0].clone();
    let mut opts = Options::new();
    opts.reqopt("q", "query", "outpack query (required)", "latest");
    opts.reqopt("r", "root", "outpack root path (required)", ".");
    let matches = match opts.parse(&args[1..]) {
        Ok(m) => { m }
        Err(f) => {
            print_usage(&program, opts);
            panic!("{}", f.to_string())
        }
    };
    // TODO: Why do we return some here when we know there is a value? Especially after unwrapping
    (Some(matches.opt_str("r").unwrap()), Some(matches.opt_str("q").unwrap()))
}

fn main() {
    let args = env::args().collect::<Vec<_>>();
    let (root, query) = parse_args(&args);
    if root.is_some() {
        let root_path = root.unwrap();
        let cfg = outpack::config::read_config(&root_path)
            .unwrap_or_else(|error| {
                panic!("Could not open outpack root at {}: {:?}",
                       root_path, error);
            });
        println!("Query result is: {}", outpack::query::run_query(cfg, query.unwrap()));
    }
}
