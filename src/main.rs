//! `am` (and its owned alias `agent-memory`): the only executable.

use agent_memory::cli;
use agent_memory::console::Channel;
use agent_memory::process;
use std::io::Write;

fn main() {
    // Before any domain code: SIGINT/SIGTERM kill and reap owned children.
    if let Err(error) = process::install_termination_handler() {
        let _ = writeln!(
            std::io::stderr(),
            "am: cannot install signal handling: {error}"
        );
        std::process::exit(1);
    }
    if let Err(error) = cli::register_providers() {
        let _ = writeln!(std::io::stderr(), "am: {}", error.detail);
        std::process::exit(1);
    }
    let mut argv = std::env::args_os();
    let argv0 = argv.next().unwrap_or_default();
    let (code, transcript) = cli::entry(&argv0, argv.collect());
    // SIGINT during the command: every guard has dropped by now; die by the
    // signal as the baseline's uncaught KeyboardInterrupt does, printing no
    // result of the interrupted command.
    process::exit_if_interrupted();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut stdout = stdout.lock();
    let mut stderr = stderr.lock();
    let mut failed = false;
    for (channel, text) in &transcript.parts {
        let written = match channel {
            Channel::Stdout => stdout
                .write_all(text.as_bytes())
                .and_then(|()| stdout.flush()),
            Channel::Stderr => stderr
                .write_all(text.as_bytes())
                .and_then(|()| stderr.flush()),
        };
        failed |= written.is_err();
    }
    // SIGINT while printing: the baseline dies by it inside print as well.
    process::exit_if_interrupted();
    // An unwritable result (closed stdout) is a failure, never a silent success.
    std::process::exit(if failed && code == 0 { 1 } else { code });
}
