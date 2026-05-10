mod cli;
mod jiggler;
mod platform;

use std::process::ExitCode;

fn main() -> ExitCode {
    let cfg = match cli::parse(std::env::args().skip(1).collect()) {
        Ok(cli::Action::Run(c)) => c,
        Ok(cli::Action::Help) => {
            print!("{}", cli::HELP);
            return ExitCode::SUCCESS;
        }
        Ok(cli::Action::Version) => {
            println!("mouse-jiggler {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("error: {e}\n\n{}", cli::HELP);
            return ExitCode::from(2);
        }
    };

    let mouse = match platform::Mouse::new() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: failed to initialize input driver: {e}");
            return ExitCode::FAILURE;
        }
    };

    match jiggler::run(&mouse, &cfg) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
