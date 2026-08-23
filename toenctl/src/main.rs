fn main() {
    if let Err(error) = toenctl::run(std::env::args().skip(1).collect()) {
        eprintln!("toenctl: {error}");
        std::process::exit(1);
    }
}
