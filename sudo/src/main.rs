mod elevate_handler;
mod helpers;
mod logging_bindings;
mod messages;
mod r;
mod rpc_bindings;
mod rpc_bindings_client;
mod rpc_bindings_server;
mod run_handler;
mod tests;
mod tracing;

use elevate_handler::start_rpc_server;
use helpers::*;
use run_handler::run_target;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt::Write;
use tracing::*;
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        System::{Console::*, Registry::HKEY_LOCAL_MACHINE},
    },
};

#[derive(Debug, Default, PartialEq, Eq)]
struct RunOptions {
    copy_env: bool,
    mode: Option<SudoMode>,
    chdir: Option<OsString>,
    commandline: Vec<OsString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelpTopic {
    Root,
    Run,
    Config,
    Elevate,
    Help,
}

#[derive(Debug, PartialEq, Eq)]
enum CliAction {
    Run(RunOptions),
    Config(Option<SudoMode>),
    Elevate {
        parent: u32,
        nonce: u32,
        commandline: Vec<OsString>,
    },
    Help {
        topic: HelpTopic,
        long: bool,
        code: i32,
    },
    Version,
}

#[derive(Debug)]
struct CliError {
    topic: HelpTopic,
    error: lexopt::Error,
}

fn parse_args(
    args: impl IntoIterator<Item = impl Into<OsString>>,
) -> std::result::Result<CliAction, CliError> {
    let mut parser = lexopt::Parser::from_args(args);
    let mut topic = HelpTopic::Root;
    parse_cli(&mut parser, &mut topic).map_err(|error| CliError { topic, error })
}

fn option_value(parser: &mut lexopt::Parser) -> std::result::Result<OsString, lexopt::Error> {
    if let Some(value) = parser.optional_value() {
        return Ok(value);
    }
    // Unlike lexopt's value(), clap did not consume a separate option as a value.
    if let Some(value) = parser
        .try_raw_args()
        .and_then(|args| args.peek().map(OsStr::to_owned))
    {
        if value != "-" && value.as_encoded_bytes().starts_with(b"-") {
            return Err(lexopt::Error::UnexpectedArgument(value));
        }
    }
    parser.value()
}

fn parse_cli(
    parser: &mut lexopt::Parser,
    topic: &mut HelpTopic,
) -> std::result::Result<CliAction, lexopt::Error> {
    use lexopt::Arg::*;
    use lexopt::ValueExt;

    let mut run = RunOptions::default();
    let mut enable = None;
    let mut parent = None;
    let mut nonce = None;
    let mut conflict = None;
    let mut seen_modes = 0u8;
    let mut saw_arg = false;

    loop {
        // Consume the delimiter ourselves so `sudo -- run` executes a program
        // named "run", rather than selecting the built-in subcommand.
        if let Some(mut raw) = parser.try_raw_args() {
            if raw.next_if(|arg| arg == "--").is_some() {
                run.commandline.extend(raw);
                saw_arg |= !run.commandline.is_empty();
                if *topic == HelpTopic::Config && !run.commandline.is_empty() {
                    return Err(lexopt::Error::UnexpectedArgument(run.commandline.remove(0)));
                }
                break;
            }
        }

        let Some(arg) = parser.next()? else { break };
        saw_arg = true;
        match arg {
            Short('h') | Long("help") if *topic == HelpTopic::Root => {
                let long = matches!(arg, Long(_));
                if long {
                    parser.raw_args()?;
                }
                return Ok(CliAction::Help {
                    topic: *topic,
                    long,
                    code: 0,
                });
            }
            Short('V') | Long("version") if *topic == HelpTopic::Root => {
                if matches!(arg, Long(_)) {
                    parser.raw_args()?;
                }
                return Ok(CliAction::Version);
            }
            Short('E') | Long("preserve-env")
                if matches!(topic, HelpTopic::Root | HelpTopic::Run) =>
            {
                if run.copy_env {
                    return Err(
                        "the argument '--preserve-env' cannot be used multiple times".into(),
                    );
                }
                run.copy_env = true;
            }
            Short('N') | Long("new-window") | Long("disable-input") | Long("inline")
                if matches!(topic, HelpTopic::Root | HelpTopic::Run) =>
            {
                let mode = match arg {
                    Long("disable-input") => SudoMode::DisableInput,
                    Long("inline") => SudoMode::Normal,
                    _ => SudoMode::ForceNewWindow,
                };
                let bit = 1 << mode as u8;
                if seen_modes & bit != 0 {
                    return Err("each run mode may be specified only once".into());
                }
                seen_modes |= bit;
                if run.mode.is_some() {
                    conflict = Some("only one of '--new-window', '--disable-input', or '--inline' may be specified");
                }
                run.mode = Some(mode);
            }
            Short('D') | Long("chdir") if matches!(topic, HelpTopic::Root | HelpTopic::Run) => {
                if run.chdir.is_some() {
                    return Err("the argument '--chdir' cannot be used multiple times".into());
                }
                run.chdir = Some(option_value(parser)?);
            }
            Long("enable") if *topic == HelpTopic::Config => {
                if enable.is_some() {
                    return Err("the argument '--enable' cannot be used multiple times".into());
                }
                let value = option_value(parser)?;
                enable = Some(match value.to_str() {
                    Some("disable") => SudoMode::Disabled,
                    Some("forceNewWindow") => SudoMode::ForceNewWindow,
                    Some("disableInput") => SudoMode::DisableInput,
                    Some("enable" | "normal" | "default") => SudoMode::Normal,
                    _ => return Err(format!("{} {:?}", r::IDS_INVALIDMODE.get(), value).into()),
                });
            }
            Short('p') | Short('n') if *topic == HelpTopic::Elevate => {
                let slot = if arg == Short('p') {
                    &mut parent
                } else {
                    &mut nonce
                };
                if slot.is_some() {
                    return Err(
                        "the arguments '-p' and '-n' may each be specified only once".into(),
                    );
                }
                *slot = Some(option_value(parser)?.parse::<u32>()?);
            }
            Value(value) => {
                if *topic == HelpTopic::Root {
                    let subcommand = match value.to_str() {
                        Some("run") => Some(HelpTopic::Run),
                        Some("config") => Some(HelpTopic::Config),
                        Some("elevate") => Some(HelpTopic::Elevate),
                        Some("help") => Some(HelpTopic::Help),
                        _ => None,
                    };
                    if let Some(subcommand) = subcommand {
                        if subcommand == HelpTopic::Help {
                            for name in parser.raw_args()? {
                                *topic = match (*topic, name.to_str()) {
                                    (HelpTopic::Root, Some("run")) => HelpTopic::Run,
                                    (HelpTopic::Root, Some("config")) => HelpTopic::Config,
                                    (HelpTopic::Root, Some("elevate")) => HelpTopic::Elevate,
                                    (HelpTopic::Root, Some("help")) => HelpTopic::Help,
                                    _ => return Err(lexopt::Error::UnexpectedArgument(name)),
                                };
                            }
                            return Ok(CliAction::Help {
                                topic: *topic,
                                long: true,
                                code: 0,
                            });
                        }
                        *topic = subcommand;
                        // These options are scoped to each command, not global.
                        run = RunOptions::default();
                        seen_modes = 0;
                        saw_arg = false;
                        continue;
                    }
                }
                if *topic == HelpTopic::Config {
                    return Err(lexopt::Error::UnexpectedArgument(value));
                }
                run.commandline.push(value);
                run.commandline.extend(parser.raw_args()?);
                break;
            }
            _ => return Err(arg.unexpected()),
        }
    }

    if !saw_arg && matches!(topic, HelpTopic::Root | HelpTopic::Run) {
        return Ok(CliAction::Help {
            topic: *topic,
            long: false,
            code: 2,
        });
    }
    if let Some(conflict) = conflict {
        return Err(conflict.into());
    }
    match topic {
        HelpTopic::Config => Ok(CliAction::Config(enable)),
        HelpTopic::Elevate => Ok(CliAction::Elevate {
            parent: parent.ok_or(lexopt::Error::MissingValue {
                option: Some("-p".into()),
            })?,
            nonce: nonce.ok_or(lexopt::Error::MissingValue {
                option: Some("-n".into()),
            })?,
            commandline: run.commandline,
        }),
        _ => Ok(CliAction::Run(run)),
    }
}

fn usage(topic: HelpTopic) -> &'static str {
    match topic {
        HelpTopic::Root => "sudo [OPTIONS] [COMMANDLINE]... [COMMAND]",
        HelpTopic::Run => "sudo run [OPTIONS] [COMMANDLINE]...",
        HelpTopic::Config => "sudo config [OPTIONS]",
        HelpTopic::Elevate => "sudo elevate -p <PARENT> -n <NONCE> [COMMANDLINE]...",
        HelpTopic::Help => "sudo help [COMMAND]...",
    }
}

fn help_text(config: &PolicyConfig, topic: HelpTopic, long: bool) -> String {
    if topic == HelpTopic::Root {
        if config.policy_mode == SudoMode::Disabled {
            return format!("{}\n", r::IDS_DISABLEDBYPOLICY.get());
        }
        if config.allowed_mode == SudoMode::Disabled {
            return format!("{}\n", r::IDS_DISABLEDLONGABOUT.get());
        }
    }
    let about = match topic {
        HelpTopic::Root if long => r::IDS_LONGABOUT.get(),
        HelpTopic::Root => r::IDS_SUDONAME.get(),
        HelpTopic::Run => r::IDS_RUN_ABOUT.get(),
        HelpTopic::Config => r::IDS_CONFIG_ABOUT.get(),
        HelpTopic::Elevate => r::IDS_ELEVATE_ABOUT.get(),
        HelpTopic::Help => "Print this message or the help of the given subcommand(s)",
    };
    let mut text = format!("{about}\n\nUsage: {}\n", usage(topic));
    if topic == HelpTopic::Root {
        _ = writeln!(text, "\nCommands:\n  run     {}\n  config  {}\n  help    Print this message or the help of the given subcommand(s)",
            r::IDS_RUN_ABOUT.get(), r::IDS_CONFIG_ABOUT.get());
    }
    if matches!(topic, HelpTopic::Root | HelpTopic::Run | HelpTopic::Elevate) {
        let description = if topic == HelpTopic::Elevate {
            r::IDS_ELEVATE_COMMANDLINE.get()
        } else {
            r::IDS_RUN_COMMANDLINE_HELP.get()
        };
        _ = writeln!(text, "\nArguments:\n  [COMMANDLINE]...  {description}");
    }
    if topic == HelpTopic::Help {
        text.push_str("\nArguments:\n  [COMMAND]...  Print help for the subcommand(s)\n");
        return text;
    }
    text.push_str("\nOptions:\n");
    if matches!(topic, HelpTopic::Root | HelpTopic::Run) {
        for (option, description) in [
            ("-E, --preserve-env", r::IDS_RUN_COPYENV_HELP.get()),
            ("-N, --new-window", r::IDS_RUN_NEWWINDOW_HELP.get()),
            ("    --disable-input", r::IDS_RUN_DISABLEINPUT_HELP.get()),
            ("    --inline", r::IDS_RUN_INLINE_HELP.get()),
            ("-D, --chdir <chdir>", r::IDS_RUN_CHDIR_HELP.get()),
        ] {
            _ = writeln!(text, "  {option:<19}  {description}");
        }
    }
    match topic {
        HelpTopic::Root => {
            let help = if long { r::IDS_BASE_HELP_HELP_LONG.get() } else { r::IDS_BASE_HELP_HELP_SHORT.get() };
            _ = writeln!(text, "  -h, --help           {help}\n  -V, --version        {}", r::IDS_BASE_VERSION_HELP.get());
        }
        HelpTopic::Config => text.push_str("      --enable <enable>  [possible values: disable, enable, forceNewWindow, disableInput, normal, default]\n"),
        HelpTopic::Elevate => {
            _ = writeln!(text, "  -p <PARENT>  {}\n  -n <NONCE>", r::IDS_ELEVATE_PARENT.get());
        }
        _ => {}
    }
    text
}

fn print_help(config: &PolicyConfig, topic: HelpTopic, long: bool, code: i32) {
    let text = help_text(config, topic, long);
    let mode =
        (topic == HelpTopic::Root && config.allowed_mode == SudoMode::Disabled).then(enable_vt);
    if code == 0 {
        print!("{text}");
    } else {
        eprint!("{text}");
    }
    if let Some(mode) = mode {
        _ = restore_console_mode(mode);
    }
}

fn log_modes(config: &PolicyConfig, requested_mode: Option<SudoMode>) {
    // Trace "disabled" as "they didn't pass a mode manually".
    trace_modes(
        requested_mode.unwrap_or(SudoMode::Disabled) as u32,
        config.setting_mode as u32,
        config.policy_mode as u32,
    );
}

fn check_enabled_or_bail(config: &PolicyConfig) -> SudoMode {
    // First things first: Make sure we're enabled.
    if config.policy_mode == SudoMode::Disabled {
        eprintln!("{}", r::IDS_DISABLEDBYPOLICY.get());
        std::process::exit(E_ACCESS_DISABLED_BY_POLICY.0);
    }

    if config.allowed_mode == SudoMode::Disabled {
        // Any time you want to use IDS_DISABLEDLONGABOUT, make sure you turned on VT first
        let mode = enable_vt();
        eprintln!("{}", r::IDS_DISABLEDLONGABOUT.get());
        _ = restore_console_mode(mode);
        std::process::exit(E_ACCESSDENIED.0);
    }

    config.allowed_mode
}

/// Try to enable VT processing in the console, but also ignore any errors.
fn enable_vt() -> CONSOLE_MODE {
    enable_vt_or_err().unwrap_or_default()
}

fn enable_vt_or_err() -> Result<CONSOLE_MODE> {
    unsafe {
        let mut console_mode = CONSOLE_MODE::default();
        let console_handle = GetStdHandle(STD_OUTPUT_HANDLE)?;
        GetConsoleMode(console_handle, &mut console_mode)?;
        SetConsoleMode(
            console_handle,
            console_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING,
        )?;
        Ok(console_mode)
    }
}
fn restore_console_mode(mode: CONSOLE_MODE) -> Result<()> {
    unsafe {
        let console_handle = GetStdHandle(STD_OUTPUT_HANDLE)?;
        SetConsoleMode(console_handle, mode)?;
    }
    Ok(())
}

fn main() {
    trace_log_message(&format!("raw commandline: {:?}", env::args_os()));

    let res =
        PolicyConfig::from_registry().and_then(|config| match parse_args(env::args_os().skip(1)) {
            Ok(CliAction::Run(options)) => do_run(&config, options),
            Ok(CliAction::Config(mode)) => do_config(&config, mode),
            Ok(CliAction::Elevate {
                parent,
                nonce,
                commandline,
            }) => do_elevate(&config, parent, nonce, &commandline),
            Ok(CliAction::Help { topic, long, code }) => {
                print_help(&config, topic, long, code);
                Ok(code)
            }
            Ok(CliAction::Version) => {
                println!("{} {}", env!("CARGO_CRATE_NAME"), env!("CARGO_PKG_VERSION"));
                Ok(0)
            }
            Err(err) => {
                eprintln!(
                    "error: {}\n\nUsage: {}\n\nFor more information, try 'help'.",
                    err.error,
                    usage(err.topic)
                );
                Ok(2)
            }
        });

    let code = res.unwrap_or_else(|err| {
        let hr = err.code();
        let mut code = hr.0;
        match hr {
            E_DIR_BAD_COMMAND_OR_FILE => {
                eprintln!("{}", r::IDS_COMMANDNOTFOUND.get());
                code = MSG_DIR_BAD_COMMAND_OR_FILE.0 as i32;
            }
            E_CANCELLED => {
                eprintln!("{}", r::IDS_CANCELLED.get());
            }
            _ if hr == HRESULT::from_win32(ERROR_REQUEST_REFUSED.0) => {
                eprintln!("{}", r::IDS_SUDO_DISALLOWED.get());
            }
            _ => {
                eprintln!("{} {}", r::IDS_UNKNOWNERROR.get(), err);
            }
        };
        code
    });

    // Normally this is where we'd construct an ExitCode and return it from main(),
    // but it only supports u8 (...why?) and windows_process_exit_code_from is an unstable feature.
    std::process::exit(code)
}

fn do_run(config: &PolicyConfig, options: RunOptions) -> Result<i32> {
    let commandline = &options.commandline;

    // Didn't pass a commandline or just "/?"? Print the help text and bail, BEFORE checking the mode.
    if commandline.is_empty() || (commandline.len() == 1 && commandline[0] == "/?") {
        print_help(config, HelpTopic::Run, true, 0);
        // return exit status 1 if the commandline was empty, 0 otherwise
        return Ok(commandline.is_empty().into());
    }
    let requested_dir = options.chdir;
    let allowed_mode = check_enabled_or_bail(config);
    let copy_env = options.copy_env;

    if !can_current_user_elevate()? {
        // Bail out with an error. main(0) will then print the error message to
        // the user to let them know they aren't allowed to run sudo.
        return Err(ERROR_REQUEST_REFUSED.into());
    }

    let requested_mode = options.mode;

    log_modes(config, requested_mode);

    if let Some(mode) = requested_mode {
        if mode > allowed_mode {
            match allowed_mode {
                SudoMode::Disabled => {} // This is already handled by check_enabled_or_bail
                SudoMode::ForceNewWindow => eprintln!("{}", r::IDS_MAXRUNMODE_FORCENEWWINDOW.get()),
                SudoMode::DisableInput => eprintln!("{}", r::IDS_MAXRUNMODE_DISABLEINPUT.get()),
                SudoMode::Normal => {} // not possible to exceed normal mode
            }
            std::process::exit(-1);
        }
    }

    let actual_mode = std::cmp::min(allowed_mode, requested_mode.unwrap_or(allowed_mode));

    run_target(copy_env, commandline, actual_mode, requested_dir)
}

fn do_elevate(
    config: &PolicyConfig,
    parent_pid: u32,
    nonce: u32,
    commandline: &[OsString],
) -> Result<i32> {
    _ = check_enabled_or_bail(config);

    trace_log_message(&format!("elevate starting for parent: {parent_pid:?}"));

    let result = start_rpc_server(parent_pid, nonce, None, commandline);
    trace_log_message(&format!("elevate result: {result:?}"));
    result
}

fn do_config(config: &PolicyConfig, requested_mode: Option<SudoMode>) -> Result<i32> {
    let mode = match requested_mode {
        Some(mode) => {
            try_enable_sudo(config, mode)?;
            mode
        }
        None => check_enabled_or_bail(config),
    };

    match mode {
        SudoMode::Disabled => println!("{}", r::IDS_DISABLEDMESSAGE.get()),
        SudoMode::ForceNewWindow => println!("{}", r::IDS_CURRENTMODE_FORCENEWWINDOW.get()),
        SudoMode::DisableInput => println!("{}", r::IDS_CURRENTMODE_DISABLEINPUT.get()),
        SudoMode::Normal => println!("{}", r::IDS_CURRENTMODE_INLINE.get()),
    }

    Ok(0)
}

fn try_enable_sudo(config: &PolicyConfig, requested_mode: SudoMode) -> Result<()> {
    let elevated = is_running_elevated()?;
    if !elevated {
        eprintln!("{}", r::IDS_REQUIREADMINTOCONFIG.get());
        std::process::exit(-1);
    }
    let max_mode = config.policy_mode;
    if requested_mode > max_mode {
        match max_mode {
            SudoMode::Disabled => eprintln!("{}", r::IDS_DISABLEDBYPOLICY.get()),
            SudoMode::ForceNewWindow => eprintln!("{}", r::IDS_MAXPOLICYMODE_FORCENEWWINDOW.get()),
            SudoMode::DisableInput => eprintln!("{}", r::IDS_MAXPOLICYMODE_DISABLEINPUT.get()),
            SudoMode::Normal => eprintln!("{}", r::IDS_MAXPOLICYMODE_INLINE.get()),
        }
        std::process::exit(-1);
    }

    let result = reg_set_u32(
        HKEY_LOCAL_MACHINE,
        w!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Sudo"),
        w!("Enabled"),
        requested_mode.into(),
    );
    if let Err(err) = result {
        eprintln!("{} {}", r::IDS_ERRORSETTINGMODE.get(), err);
        return Err(err);
    }

    Ok(())
}
