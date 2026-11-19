use std::ffi::OsString;

use clap::{Arg, ArgAction, Command, value_parser};

/// The command `units::dispatch` gets when argv names no subcommand.
pub const ROOT: &str = "prettier";

fn rest() -> Arg {
    Arg::new("args")
        .num_args(0..)
        .trailing_var_arg(true)
        .allow_hyphen_values(true)
        .value_parser(value_parser!(OsString))
}

/// Routes argv to a command. Anything this parser rejects runs the original, so it never
/// has to match every way the original accepts its arguments.
pub fn command() -> Command {
    Command::new(ROOT)
        .disable_help_flag(true)
        .disable_version_flag(true)
        .arg(
            Arg::new("arrow-parens")
                .long("arrow-parens")
                .action(ArgAction::Count),
        )
        .arg(
            Arg::new("bracket-same-line")
                .long("bracket-same-line")
                .action(ArgAction::Count),
        )
        .arg(
            Arg::new("experimental-operator-position")
                .long("experimental-operator-position")
                .action(ArgAction::Count),
        )
        .arg(
            Arg::new("experimental-ternaries")
                .long("experimental-ternaries")
                .action(ArgAction::Count),
        )
        .arg(
            Arg::new("no-bracket-spacing")
                .long("no-bracket-spacing")
                .action(ArgAction::Count),
        )
        .arg(Arg::new("no-semi").long("no-semi").action(ArgAction::Count))
        .arg(
            Arg::new("object-wrap")
                .long("object-wrap")
                .action(ArgAction::Count),
        )
        .arg(
            Arg::new("print-width")
                .long("print-width")
                .action(ArgAction::Count),
        )
        .arg(
            Arg::new("quote-props")
                .long("quote-props")
                .action(ArgAction::Count),
        )
        .arg(
            Arg::new("single-quote")
                .long("single-quote")
                .action(ArgAction::Count),
        )
        .arg(
            Arg::new("stdin-filepath")
                .long("stdin-filepath")
                .action(ArgAction::Count),
        )
        .arg(
            Arg::new("tab-width")
                .long("tab-width")
                .action(ArgAction::Count),
        )
        .arg(
            Arg::new("trailing-comma")
                .long("trailing-comma")
                .action(ArgAction::Count),
        )
        .arg(
            Arg::new("use-tabs")
                .long("use-tabs")
                .action(ArgAction::Count),
        )
        .arg(rest())
}
