fn main() {
    if let Err(error) = libc_support_tools::runtime_tools::main_from_env() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
