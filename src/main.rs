// use std::io::{self, BufRead};

// _direnv_hook () {
//     trap -- '' SIGINT
//     eval "$(/Users/b.caldwell/code/src/github.com/bcaldwell/direnv-pretty/target/debug/direnv-pretty)"
//     trap - SIGINT
// }
//     eval "$(direnv export zsh 2> >( /Users/b.caldwell/code/src/github.com/bcaldwell/direnv-pretty/target/debug/direnv-pretty ))"
//     eval "$("/nix/store/nqsbh35psklpnlv27zrqshn9vfmjdqdc-direnv-2.30.3/bin/direnv" export zsh | /Users/b.caldwell/code/src/github.com/bcaldwell/direnv-pretty/target/debug/direnv-pretty)"
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

// stealing from https://github.com/Shopify/shadowenv/blob/b4c8979f3a80fd6152e836594a66563441bbf4d8/src/output.rs
// "direnv" in a gradient of lighter to darker grays. Looks good on dark backgrounds and ok on
// light backgrounds.
const DIRENV: &'static str = concat!(
    "\x1b[38;5;249md\x1b[38;5;248mi\x1b[38;5;247mr\x1b[38;5;246me\x1b[38;5;245mn",
    "\x1b[38;5;244mv\x1b[38;5;240m",
);

const LONG_EXEC_TIME: u32 = 300;
const MS_TO_S: f32 = 1000.0;
// const FEATURE_PREFIX:String = "use ".to_string();

fn main() {
    // let stdin = io::stdin();
    // for line in stdin.lock().lines() {
    //     let line = line.expect("Could not read line from standard in");
    //     eprintln!("Captured line: {}", line);
    // }
    let now = Instant::now();
    let cmd = Command::new("direnv");
    let output = Command::new("direnv")
        .arg("export")
        .arg("zsh")
        // connect stdin
        .stdin(Stdio::piped())
        .output()
        .expect("failed to execute process");
    // forward stdout as is
    println!(
        "{}",
        String::from_utf8(output.stdout).expect("failed to get stdout")
    );

    // update stderr to be pretty
    let stderr = String::from_utf8(output.stderr).expect("failed to get stdout");
    let elapsed_time = now.elapsed();
    let time_str = if elapsed_time.subsec_millis() > LONG_EXEC_TIME {
        format!(" ({:.2}s)", elapsed_time.subsec_millis() as f32 / MS_TO_S)
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

        eprintln!("\x1b[1;34mactivated {}{}{}", DIRENV, features_str, time_str);
    } else if stderr.contains("direnv: unloading") {
        eprintln!("\x1b[1;34mdeactivated {}{}", DIRENV, time_str);
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
