// _direnv_hook () {
//     trap -- '' SIGINT
//     eval "$(/Users/b.caldwell/code/src/github.com/bcaldwell/direnv-pretty/target/debug/direnv-pretty)"
//     trap - SIGINT
// }
//     eval "$(direnv export zsh 2> >( /Users/b.caldwell/code/src/github.com/bcaldwell/direnv-pretty/target/debug/direnv-pretty ))"
//     eval "$("/nix/store/nqsbh35psklpnlv27zrqshn9vfmjdqdc-direnv-2.30.3/bin/direnv" export zsh | /Users/b.caldwell/code/src/github.com/bcaldwell/direnv-pretty/target/debug/direnv-pretty)"
use anyhow::{Result, Context};
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

const DEBUG_MODE_ENV: &'static str = "PRETTY_DIRENV_DEBUG";
const SILENT_MODE_ENV: &'static str = "PRETTY_DIRENV_SILENT";

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

fn main() -> Result<()> {
    let args = Args::parse();

    if args.args.len() == 0 {
        return Ok(());
    }

    match args.args[0].as_str() {
        "export" => run_export(args).context("failed to run export command"),
        "hook" => run_hook(args).context("failed to run hook command"),
        _ => run_default(args).context("fauled to run default command"),
    }?;
    Ok(())
}

fn run_default(args: Args) -> Result<()> {
    args.build_command().status()?;
    Ok(())
}

fn run_hook(args: Args) -> Result<()> {
    let output = args.build_command().output().context("failed to run direnv")?;

    // forward stderr as is
    println!(
        "{}",
        String::from_utf8(output.stderr).context("failed to get stdout")?
    );

    let stdout = String::from_utf8(output.stdout).context("failed to get stdout")?;

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
    Ok(())
}

fn env_var_true(name: &str) -> bool {
    match env::var(name) {
        Ok(val) if val == "true" => true,
        _ => false,
    }
}

fn run_export(args: Args) -> Result<()> {
    let silent_output = env_var_true(SILENT_MODE_ENV);
    let mut output_stream: Box<dyn std::io::Write> = match silent_output {
        true => Box::new(io::sink()),
        false => Box::new(io::stderr()),
    };

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
        if let Some(_) = cmd.try_wait()? {
            break;
        }

        // Sleep for a short duration
        thread::sleep(Duration::from_millis(20));
    }

    let spinner = if let Some(_) = cmd.try_wait()? {
        None
    } else {
        if silent_output {
            None
        } else {
            Some(Spinner::with_timer_and_stream(
                Spinners::Dots,
                "loading environment".into(),
                spinners::Stream::Stderr,
            ))
        }
    };

    let output = cmd.wait_with_output()?;

    if let Some(mut spinner) = spinner {
        spinner.stop_with_message("".into());
        // remove new line
        eprint!("\x1b[1A");
    }

    // forward stdout as is
    println!("{}", String::from_utf8(output.stdout)?);

    let stderr = String::from_utf8(output.stderr)?;

    let has_error = output.status.code() != Some(0) || output_has_error(&stderr);
    let debug_mode = env_var_true(DEBUG_MODE_ENV) || has_error;

    if debug_mode {
        eprintln!("{}", &stderr);
    }
    // update stderr to be pretty
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

        write!(
            output_stream,
            "{} {}{}{}{}",
            action, DIRENV, features_str, time_str, COLOR_RESET
        )?;
    } else if stderr.contains("direnv: unloading") {
        let action = if has_error {
            "\x1b[1;31mfailed deactivating".to_string()
        } else {
            "\x1b[1;34mdeactivated".to_string()
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

fn output_has_error(output: &str) -> bool {
    for line in output.lines() {
        if line.starts_with("error: ") {
            return true;
        }
    }

    return false;
}
