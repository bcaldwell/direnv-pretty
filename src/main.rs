// _direnv_hook () {
//     trap -- '' SIGINT
//     eval "$(/Users/b.caldwell/code/src/github.com/bcaldwell/direnv-pretty/target/debug/direnv-pretty)"
//     trap - SIGINT
// }
//     eval "$(direnv export zsh 2> >( /Users/b.caldwell/code/src/github.com/bcaldwell/direnv-pretty/target/debug/direnv-pretty ))"
//     eval "$("/nix/store/nqsbh35psklpnlv27zrqshn9vfmjdqdc-direnv-2.30.3/bin/direnv" export zsh | /Users/b.caldwell/code/src/github.com/bcaldwell/direnv-pretty/target/debug/direnv-pretty)"
use spinners::{Spinner, Spinners};
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::{env, thread};
use which::which;

use clap::Parser;

// stealing from https://github.com/Shopify/shadowenv/blob/b4c8979f3a80fd6152e836594a66563441bbf4d8/src/output.rs
// "direnv" in a gradient of lighter to darker grays. Looks good on dark backgrounds and ok on
// light backgrounds.
const DIRENV: &'static str = concat!(
    "\x1b[38;5;249md\x1b[38;5;248mi\x1b[38;5;247mr\x1b[38;5;246me\x1b[38;5;245mn",
    "\x1b[38;5;244mv\x1b[38;5;240m",
);
const COLOR_RESET: &'static str = "\x1b[0m";

const LONG_EXEC_TIME: u128 = 250;
const MS_TO_S: f32 = 1000.0;
// const FEATURE_PREFIX:String = "use ".to_string();

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    #[clap(short, long)]
    direnv: Option<String>,
    args: Vec<String>,
}

impl Args {
    fn resolve_direnv_path(&self) -> String {
        let default_direnv = "direnv";
        return self
            .direnv
            .as_ref()
            .unwrap_or(&default_direnv.to_string())
            .to_string();
    }
    fn build_command(&self) -> Command {
        let mut cmd = Command::new(self.resolve_direnv_path());
        cmd.args(&self.args)
            // connect stdin, only care about stdout/stderr
            .stdin(Stdio::piped());

        return cmd;
    }
}

fn main() {
    let args = Args::parse();

    if args.args.len() == 0 {
        return;
    }

    match args.args[0].as_str() {
        "export" => run_export(args),
        "hook" => run_hook(args),
        _ => run_default(args),
    };
}

fn run_default(args: Args) {
    args.build_command().status().expect("failed to run direnv");
}

fn run_hook(args: Args) {
    let output = args.build_command().output().expect("failed to run direnv");

    // forward stderr as is
    println!(
        "{}",
        String::from_utf8(output.stderr).expect("failed to get stdout")
    );

    let stdout = String::from_utf8(output.stdout).expect("failed to get stdout");

    // detect current direnv path and replace it with the pretty version
    // pass the current path in as a flag --direnv
    let direnv_path = which(args.resolve_direnv_path())
        .unwrap()
        .into_os_string()
        .into_string()
        .unwrap();
    let direnv_pretty_path = env::current_exe()
        .unwrap()
        .into_os_string()
        .into_string()
        .unwrap();
    let updated_output = stdout.replace(
        &format!("\"{}\"", direnv_path),
        &format!("\"{}\" --direnv {}", direnv_pretty_path, direnv_path),
    );

    println!("{}", updated_output);
}

fn run_export(args: Args) {
    let now = Instant::now();
    let mut cmd = args
        .build_command()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // only show the spinner for long running command runs
    loop {
        if now.elapsed().as_millis() >= LONG_EXEC_TIME {
            break;
        }
        if let Some(_) = cmd.try_wait().expect("failed to execute process") {
            break;
        }

        // Sleep for a short duration
        thread::sleep(Duration::from_millis(20));
    }

    let spinner = if let Some(_) = cmd.try_wait().expect("failed to execute process") {
        None
    } else {
        Some(Spinner::with_timer_and_stream(
            Spinners::Dots,
            "loading environment".into(),
            spinners::Stream::Stderr,
        ))
    };

    let output = cmd.wait_with_output().expect("failed to execute process");

    if let Some(mut spinner) = spinner {
        spinner.stop_with_message("".into());
        // remove new line
        eprint!("\x1b[1A");
    }

    // forward stdout as is
    println!(
        "{}",
        String::from_utf8(output.stdout).expect("failed to get stdout")
    );

    let stderr = String::from_utf8(output.stderr).expect("failed to get stdout");

    let has_error = output.status.code() != Some(0) || output_has_error(&stderr);
    let debug_mode = env::var("PRETTY_DIRENV_DEBUG").is_ok() || has_error;

    // update stderr to be pretty
    if debug_mode {
        eprintln!("{}", &stderr);
    }
    let elapsed_time = now.elapsed();
    let time_str = if elapsed_time.as_millis() > LONG_EXEC_TIME {
        format!(" ({:.2}s)", elapsed_time.as_millis() as f32 / MS_TO_S)
    } else {
        "".to_string()
    };

    let mut features = Vec::new();
    if stderr.contains("direnv: loading") {
        if let Ok(lines) = read_lines("./.envrc") {
            // Consumes the iterator, returns an (Optional) String
            for line in lines {
                if let Ok(feature_line) = line {
                    if feature_line.starts_with("use ") {
                        features.push(feature_line.trim_start_matches("use ").to_string())
                    }
                }
            }
        }

        let features_str = if features.len() > 0 {
            format!(" ({})", features.join(", "))
        } else {
            "".to_string()
        };

        let action = if has_error {
            "\x1b[1;31mfailed activating".to_string()
        } else {
            "\x1b[1;34mactivated".to_string()
        };

        eprintln!(
            "{} {}{}{}{}",
            action, DIRENV, features_str, time_str, COLOR_RESET
        );
    } else if stderr.contains("direnv: unloading") {
        let action = if has_error {
            "\x1b[1;31mfailed deactivating".to_string()
        } else {
            "\x1b[1;34mdeactivated".to_string()
        };
        eprintln!("{} {}{}{}", action, DIRENV, time_str, COLOR_RESET);
    }
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

fn output_has_error(output: &str) -> bool {
    for line in output.lines() {
        if line.starts_with("error: ") {
            return true;
        }
    }

    return false;
}
