fn main() {
    if let Err(error) = libc_support_tools::network_tools::main_from_env() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
