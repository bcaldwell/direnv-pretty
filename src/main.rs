// _direnv_hook () {
//     trap -- '' SIGINT
//     eval "$(/Users/b.caldwell/code/src/github.com/bcaldwell/direnv-pretty/target/debug/direnv-pretty)"
//     trap - SIGINT
// }
//     eval "$(direnv export zsh 2> >( /Users/b.caldwell/code/src/github.com/bcaldwell/direnv-pretty/target/debug/direnv-pretty ))"
//     eval "$("/nix/store/nqsbh35psklpnlv27zrqshn9vfmjdqdc-direnv-2.30.3/bin/direnv" export zsh | /Users/b.caldwell/code/src/github.com/bcaldwell/direnv-pretty/target/debug/direnv-pretty)"
use anyhow::{Context, Result};
use spinners::{Spinner, Spinners};
use std::fs::File;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use std::{env, thread};
use which::which;

use clap::Parser;

// stealing from https://github.com/Shopify/shadowenv/blob/b4c8979f3a80fd6152e836594a66563441bbf4d8/src/output.rs
// "direnv" in a gradient of lighter to darker grays. Looks good on dark backgrounds and ok on
// light backgrounds.
const DIRENV: &str = concat!(
    "\x1b[38;5;249md\x1b[38;5;248mi\x1b[38;5;247mr\x1b[38;5;246me\x1b[38;5;245mn",
    "\x1b[38;5;244mv\x1b[38;5;240m",
);
const COLOR_RESET: &str = "\x1b[0m";

const LONG_EXEC_TIME: Duration = Duration::from_millis(250);

const DEBUG_MODE_ENV: &str = "PRETTY_DIRENV_DEBUG";
const SILENT_MODE_ENV: &str = "PRETTY_DIRENV_SILENT";

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Path to the real direnv binary (defaults to `direnv` on PATH)
    #[clap(short, long)]
    direnv: Option<String>,
    args: Vec<String>,
}

impl Args {
    fn resolve_direnv_path(&self) -> &str {
        self.direnv.as_deref().unwrap_or("direnv")
    }

    fn build_command(&self) -> Command {
        let mut cmd = Command::new(self.resolve_direnv_path());
        // pass stdin through so .envrc scripts can still read from the terminal
        cmd.args(&self.args).stdin(Stdio::inherit());
        cmd
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.args.first().map(String::as_str) {
        None => Ok(()),
        Some("export") => run_export(args).context("failed to run export command"),
        Some("hook") => run_hook(args).context("failed to run hook command"),
        Some(_) => run_default(args).context("failed to run command"),
    }
}

fn run_default(args: Args) -> Result<()> {
    let status = args.build_command().status()?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn run_hook(args: Args) -> Result<()> {
    let output = args
        .build_command()
        .output()
        .context("failed to run direnv")?;

    // forward stderr as is (stdout is eval'd by the shell, so it must stay clean)
    io::stderr().write_all(&output.stderr)?;

    let stdout = String::from_utf8(output.stdout).context("direnv stdout was not utf-8")?;

    // detect current direnv path and replace it with the pretty version
    // pass the current path in as a flag --direnv
    let direnv_path = which(args.resolve_direnv_path()).context("could not find direnv")?;
    let direnv_path = direnv_path.to_string_lossy();
    let direnv_pretty_path = env::current_exe().context("could not find current exe")?;
    let direnv_pretty_path = direnv_pretty_path.to_string_lossy();
    // direnv <= 2.37 quotes its path as "path"; 2.38 uses $'path'
    let mut updated_output = stdout;
    for (open, close) in [("\"", "\""), ("$'", "'")] {
        updated_output = updated_output.replace(
            &format!("{open}{direnv_path}{close}"),
            &format!("{open}{direnv_pretty_path}{close} --direnv {open}{direnv_path}{close}"),
        );
    }

    print!("{}", updated_output);
    Ok(())
}

fn env_var_true(name: &str) -> bool {
    matches!(env::var(name), Ok(val) if val == "true")
}

fn run_export(args: Args) -> Result<()> {
    let silent_output = env_var_true(SILENT_MODE_ENV);
    let mut output_stream: Box<dyn std::io::Write> = match silent_output {
        true => Box::new(io::sink()),
        false => Box::new(io::stderr()),
    };

    let now = Instant::now();
    let child = args
        .build_command()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn direnv")?;

    // Drain the pipes on another thread so a large export can't block direnv on a
    // full pipe, which would also make fast runs look slow.
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });

    // only show the spinner for long running command runs
    let (output, elapsed_time) = match rx.recv_timeout(LONG_EXEC_TIME) {
        Ok(output) => (output, now.elapsed()),
        Err(_) => {
            let spinner = if silent_output {
                None
            } else {
                Some(Spinner::with_timer_and_stream(
                    Spinners::Dots,
                    "loading environment".into(),
                    spinners::Stream::Stderr,
                ))
            };
            let output = rx.recv().context("direnv wait thread died")?;
            if let Some(mut spinner) = spinner {
                spinner.stop_with_message("".into());
                // remove new line
                eprint!("\x1b[1A");
            }
            (output, now.elapsed())
        }
    };
    let output = output?;

    // forward stdout as is
    let stdout = String::from_utf8(output.stdout)?;
    print!("{}", stdout);

    let stderr = String::from_utf8(output.stderr)?;

    let has_error = !output.status.success() || output_has_error(&stderr);
    let debug_mode = env_var_true(DEBUG_MODE_ENV) || has_error;

    if debug_mode {
        eprintln!("{}", stderr);
    }
    // update stderr to be pretty
    let time_str = if elapsed_time > LONG_EXEC_TIME {
        format!(" ({:.2}s)", elapsed_time.as_secs_f32())
    } else {
        "".to_string()
    };

    let mut features = Vec::new();
    if stderr.contains("direnv: loading") {
        if let Ok(lines) = read_lines("./.envrc") {
            for line in lines.map_while(Result::ok) {
                if let Some(feature) = line.strip_prefix("use ") {
                    features.push(feature.to_string());
                }
            }
        }

        let features_str = if !features.is_empty() {
            format!(" ({})", features.join(", "))
        } else {
            "".to_string()
        };

        let action = if has_error {
            "\x1b[1;31mfailed activating"
        } else {
            "\x1b[1;34mactivated"
        };

        write!(
            output_stream,
            "{} {}{}{}{}",
            action, DIRENV, features_str, time_str, COLOR_RESET
        )?;
    } else if stderr.contains("direnv: unloading") {
        let action = if has_error {
            "\x1b[1;31mfailed deactivating"
        } else {
            "\x1b[1;34mdeactivated"
        };
        write!(
            output_stream,
            "{} {}{}{}",
            action, DIRENV, time_str, COLOR_RESET
        )?;
    }

    Ok(())
}

// The output is wrapped in a Result to allow matching on errors
// Returns an Iterator to the Reader of the lines of the file.
fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

fn output_has_error(stderr: &str) -> bool {
    // error lines are colored, so strip leading escape codes before matching
    stderr.lines().any(|line| {
        let line = line
            .trim_start_matches("\x1b[0m")
            .trim_start_matches("\x1b[31m");
        line.starts_with("error: ") || line.starts_with("direnv: error")
    })
}
