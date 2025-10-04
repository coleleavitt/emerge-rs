use clap::{Arg, ArgMatches, Command};
use std::process;

use emerge_rs::actions;

#[tokio::main]
async fn main() {
    env_logger::init();

    let app = create_app();
    let matches = app.get_matches();

    let result = run_emerge(matches).await;
    process::exit(result);
}

fn create_app() -> Command {
    Command::new("emerge")
        .version("0.1.0")
        .author("Rust Portage Team")
        .about("Package manager for Gentoo")
        .arg(
            Arg::new("ask")
                .long("ask")
                .short('a')
                .help("Prompt before performing any actions")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("pretend")
                .long("pretend")
                .short('p')
                .help("Pretend (don't do anything)")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("verbose")
                .long("verbose")
                .short('v')
                .help("Verbose output")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("quiet")
                .long("quiet")
                .short('q')
                .help("Quiet output")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("update")
                .long("update")
                .short('u')
                .help("Update packages to the best available version")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("deep")
                .long("deep")
                .short('D')
                .help("Consider the entire dependency tree")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("newuse")
                .long("newuse")
                .short('N')
                .help("Include packages with changed USE flags")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("resume")
                .long("resume")
                .short('r')
                .help("Resume interrupted operations")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("jobs")
                .long("jobs")
                .short('j')
                .help("Number of parallel build jobs")
                .value_parser(clap::value_parser!(usize))
                .default_value("1"),
        )
        .arg(
            Arg::new("with_bdeps")
                .long("with-bdeps")
                .help("Include build dependencies")
                .value_parser(["y", "n"])
                .default_value("n"),
        )
        .arg(
            Arg::new("sync")
                .long("sync")
                .help("Sync package repositories")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("packages")
                .help("Packages to operate on")
                .action(clap::ArgAction::Set)
                .num_args(0..),
        )
}

async fn run_emerge(matches: ArgMatches) -> i32 {
    let ask = matches.get_flag("ask");
    let pretend = matches.get_flag("pretend");
    let verbose = matches.get_flag("verbose");
    let update = matches.get_flag("update");
    let deep = matches.get_flag("deep");
    let newuse = matches.get_flag("newuse");
    let resume = matches.get_flag("resume");
    let jobs = matches.get_one::<usize>("jobs").copied().unwrap_or(1);
    let with_bdeps = matches.get_one::<String>("with_bdeps").map(|s| s == "y").unwrap_or(false);
    
    // Set verbose mode globally if needed
    if verbose {
        unsafe {
            std::env::set_var("EMERGE_VERBOSE", "1");
        }
    }

    if matches.get_flag("sync") {
        return actions::action_sync().await;
    }

    // Get packages
    let packages: Vec<String> = matches
        .get_many::<String>("packages")
        .unwrap_or_default()
        .cloned()
        .collect();

    if packages.is_empty() {
        eprintln!("emerge: no targets specified (use --help for usage)");
        return 1;
    }

    // Determine action based on flags
    if update {
        return actions::action_upgrade(&packages, pretend, ask, deep, newuse, with_bdeps).await;
    } else {
        return actions::action_install_with_root(&packages, pretend, ask, resume, jobs, "/", with_bdeps).await;
    }
}
