fn main() {
    let args: Result<Vec<String>, _> = std::env::args_os().skip(1).map(|arg| arg.into_string()).collect();
    let code = match args {
        Ok(args) => fish_s2pro_tts::cli::run(args),
        Err(_) => { eprintln!("Arguments must be valid UTF-8"); 2 }
    };
    std::process::exit(code);
}
