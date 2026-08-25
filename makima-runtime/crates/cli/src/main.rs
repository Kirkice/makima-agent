//! Pi Agent Rust 产品入口。
//!
//! stdout 的所有权由模式决定：RPC 模式仅写 framed-CBOR；所有启动失败和诊断只写 stderr。
//! 因此 Node/Bun launcher 可以安全地转发 stdio，而不会污染协议流。

use cli::{
    contract::{ExitCode, PRODUCT_VERSION, ParseOutcome, help_text, parse_args},
    run,
};

fn main() {
    let current_dir = match std::env::current_dir() {
        Ok(directory) => directory,
        Err(error) => exit_with(
            ExitCode::Bootstrap,
            format!("cannot determine current directory: {error}"),
        ),
    };
    let outcome = match parse_args(std::env::args().skip(1), current_dir) {
        Ok(outcome) => outcome,
        Err(error) => exit_with(ExitCode::Usage, error.message),
    };

    match outcome {
        ParseOutcome::Help => {
            print!("{}", help_text());
        }
        ParseOutcome::Version => {
            println!("{PRODUCT_VERSION}");
        }
        ParseOutcome::Run(config) => {
            if let Err((code, message)) = run(config) {
                exit_with(code, message);
            }
        }
    }
}

fn exit_with(code: ExitCode, message: String) -> ! {
    eprintln!("Rust runtime: {message}");
    std::process::exit(code as i32);
}
