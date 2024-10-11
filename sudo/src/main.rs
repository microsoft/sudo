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

use clap::{Arg, ArgAction, ArgMatches, Command};
use elevate_handler::start_rpc_server;
use helpers::*;
use run_handler::run_target;
use std::env;
use tracing::*;
use windows::{core::*, Win32::Foundation::*, Win32::System::Console::*};

// Clap does provide a nice macro for args, which defines args with a syntax
// close to what the actual help text would be. Unfortunately, we're not using
// that macro, because it doesn't play well with localization. The comments
// throughout here help show how the macro would have worked.
fn sudo_cli(allowed_mode: i32) -> Command {
    const POLICY_DENIED_LABEL: i32 = E_ACCESS_DISABLED_BY_POLICY.0;

    let mut app = Command::new(env!("CARGO_CRATE_NAME"));
    match allowed_mode {
        0 => {
            // In this case, our error message has some VT in it, so we need to
            // turn VT on before we eventually print the message. Fortunately,
            // the `check_enabled_or_bail` will make sure to enable & restore VT
            // mode before printing that error.

            // Sudo is disabled. We want to inform them when they see the help text.
            app = app
                .about(r::IDS_SUDONAME.get())
                .long_about(r::IDS_DISABLEDLONGABOUT.get())
                .override_help(r::IDS_DISABLEDLONGABOUT.get());
        }
        POLICY_DENIED_LABEL => {
            // Sudo is disabled by policy. The help text will be more specific.
            app = app
                .about(r::IDS_SUDONAME.get())
                .long_about(r::IDS_DISABLEDBYPOLICY.get())
                .override_help(r::IDS_DISABLEDBYPOLICY.get());
        }
        _ => {
            // Sudo is enabled. The help text will be standard.
            app = app
                .about(r::IDS_SUDONAME.get())
                .long_about(r::IDS_LONGABOUT.get());
        }
    }

    app = app
        .subcommand_required(false)
        .arg_required_else_help(true)
        .allow_external_subcommands(true)
        .version(env!("CARGO_PKG_VERSION"))
        .args(run_args())
        .subcommand(run_builder())
        .subcommand(
            // The elevate command is hidden, and not documented in the help text.
            Command::new("elevate")
                .about(r::IDS_ELEVATE_ABOUT.get())
                .hide(true)
                .disable_help_flag(true)
                // .arg(arg!(-p <PARENT> "Parent process ID").required(true))
                .arg(
                    Arg::new("PARENT")
                        .short('p')
                        .help(r::IDS_ELEVATE_PARENT.get())
                        .required(true),
                )
                // .arg(arg!(-n <NONCE>).required(true))
                .arg(Arg::new("NONCE").short('n').required(true))
                // .arg(arg!([COMMANDLINE] ... "")),
                .arg(
                    Arg::new("COMMANDLINE")
                        .help(r::IDS_ELEVATE_COMMANDLINE.get())
                        .action(ArgAction::Append)
                        .trailing_var_arg(true),
                ),
        )
        .disable_help_flag(true)
        .disable_version_flag(true)
        .arg(
            Arg::new("help")
                .action(ArgAction::Help)
                .short('h')
                .long("help")
                .help(r::IDS_BASE_HELP_HELP_SHORT.get())
                .long_help(r::IDS_BASE_HELP_HELP_LONG.get()),
        )
        .arg(
            Arg::new("version")
                .action(ArgAction::Version)
                .short('V')
                .long("version")
                .help(r::IDS_BASE_VERSION_HELP.get()),
        );
    let config = Command::new("config").about(r::IDS_CONFIG_ABOUT.get()).arg(
        Arg::new("enable")
            .long("enable")
            .value_parser([
                "disable",
                "enable",
                "forceNewWindow",
                "disableInput",
                "normal",
                "default",
            ])
            .default_missing_value_os("default")
            .required(false)
            .action(ArgAction::Set),
    );
    app = app.subcommand(config);

    app
}

fn run_builder() -> Command {
    Command::new("run")
        .about(r::IDS_RUN_ABOUT.get())
        .arg_required_else_help(true)
        .args(run_args())
}

fn run_args() -> Vec<clap::Arg> {
    // trailing_var_arg and allow_hyphen_values are needed to allow passing in a
    // command like `sudo netstat -ab` to work as expected, instead of having
    // the parser attempt to treat the `-ab` as args to sudo itself.
    let args = vec![
        // arg!(-E --"preserve-env"  "pass the current environment variables to the command")
        Arg::new("copyEnv")
            .short('E')
            .long("preserve-env")
            .help(r::IDS_RUN_COPYENV_HELP.get())
            .action(ArgAction::SetTrue),
        // arg!(-N --"new-window"  "Use a new window for the command.")
        Arg::new("newWindow")
            .short('N')
            .long("new-window")
            .help(r::IDS_RUN_NEWWINDOW_HELP.get())
            .action(ArgAction::SetTrue)
            .group("mode"),
        // arg!(--"disable-input"  "Disable input to the target application")
        Arg::new("disableInput")
            .long("disable-input")
            .help(r::IDS_RUN_DISABLEINPUT_HELP.get())
            .action(ArgAction::SetTrue)
            .group("mode"),
        // arg!(--"inline" "Run in the current terminal")
        Arg::new("inline")
            .long("inline")
            .help(r::IDS_RUN_INLINE_HELP.get())
            .action(ArgAction::SetTrue)
            .group("mode"),
        // arg!(--"chdir"=<DIR>  "Change the working directory to DIR before running the command.")
        Arg::new("chdir")
            .short('D')
            .long("chdir")
            .help(r::IDS_RUN_CHDIR_HELP.get())
            .action(ArgAction::Set)
            .value_name("DIR"),
        // arg!([COMMANDLINE] ... "Command-line to run")
        Arg::new("COMMANDLINE")
            .help(r::IDS_RUN_COMMANDLINE_HELP.get())
            .action(ArgAction::Append)
            .trailing_var_arg(true),
    ];

    // The following is a demo of how feature flags might work in the sudo
    // codebase. You can add a `Feature_test_flag` feature to the Dev branding
    // in cargo.toml, and then add conditionally enabled code, like so:
    //
    // if cfg!(feature = "Feature_test_flag") {
    //     args.append(&mut vec![Arg::new("setHome")
    //         .short('H')
    //         .long("set-home")
    //         .help(r::IDS_RUN_SETHOME_HELP.get())
    //         .action(ArgAction::SetTrue)]);
    // }
    args
}

fn log_modes(requested_mode: Option<SudoMode>) {
    let config: RegistryConfigProvider = Default::default();
    let setting_mode = get_setting_mode(&config).unwrap_or(SudoMode::Disabled) as u32;
    let policy_mode = {
        let policy_enabled = windows_registry::LOCAL_MACHINE
            .open("SOFTWARE\\Policies\\Microsoft\\Windows\\Sudo")
            .and_then(|key| key.get_u32("Enabled"));
        if let Err(e) = &policy_enabled {
            if e.code() == E_FILENOTFOUND {
                0xffffffff
            } else {
                0
            }
        } else {
            policy_enabled.unwrap_or(0)
        }
    };
    // Trace "disabled" as "they didn't pass a mode manually".
    trace_modes(
        requested_mode.unwrap_or(SudoMode::Disabled) as u32,
        setting_mode,
        policy_mode,
    );
}

fn check_enabled_or_bail() -> SudoMode {
    let config: RegistryConfigProvider = Default::default();
    // First things first: Make sure we're enabled.
    match get_allowed_mode(&config) {
        Err(e) => {
            if e.code() == E_ACCESSDENIED {
                // Any time you want to use IDS_DISABLEDLONGABOUT, make sure you turned on VT first
                let mode = enable_vt();
                eprintln!("{}", r::IDS_DISABLEDLONGABOUT.get());
                _ = restore_console_mode(mode);
            } else if e.code() == ERROR_ACCESS_DISABLED_BY_POLICY.into() {
                eprintln!("{}", r::IDS_DISABLEDBYPOLICY.get());
            } else {
                eprintln!("{} {}", r::IDS_UNKNOWNERROR.get(), e);
            }
            std::process::exit(e.code().0);
        }
        Ok(SudoMode::Disabled) => {
            // Any time you want to use IDS_DISABLEDLONGABOUT, make sure you turned on VT first
            let mode = enable_vt();
            eprintln!("{}", r::IDS_DISABLEDLONGABOUT.get());
            _ = restore_console_mode(mode);
            std::process::exit(E_ACCESSDENIED.0);
        }
        Ok(mode) => mode,
    }
}

/// We want to be able to conditionally control what the help text shows,
/// depending on if sudo is enabled, disabled, or disabled by policy. This
/// helper lets us do that more easily. This will return:
/// * 0 if sudo is disabled
/// * E_ACCESS_DISABLED_BY_POLICY if sudo is disabled by policy
/// * or the current mode (>0), if sudo is enabled.
fn allowed_mode_for_help() -> i32 {
    let config: RegistryConfigProvider = Default::default();
    match get_allowed_mode(&config) {
        Err(e) => {
            if e.code() == E_ACCESSDENIED {
                0
            } else if e.code() == E_ACCESS_DISABLED_BY_POLICY {
                E_ACCESS_DISABLED_BY_POLICY.0
            } else {
                0
            }
        }
        Ok(SudoMode::Disabled) => 0,
        Ok(mode) => mode.into(),
    }
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
    enable_tracing();

    trace_log_message(&format!("raw commandline: {:?}", env::args_os()));
    let mode_for_help = allowed_mode_for_help();
    let matches = sudo_cli(mode_for_help).get_matches();

    let res = match matches.subcommand() {
        Some(("elevate", sub_matches)) => do_elevate(sub_matches),
        Some(("run", sub_matches)) => do_run(sub_matches),
        Some(("config", sub_matches)) => do_config(sub_matches),
        _ => do_run(&matches),
    };

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

fn do_run(matches: &ArgMatches) -> Result<i32> {
    let commandline = matches
        .get_many::<String>("COMMANDLINE")
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    // Didn't pass a commandline or just "/?"? Print the help text and bail, BEFORE checking the mode.
    if commandline.is_empty() || (commandline.len() == 1 && commandline[0] == "/?") {
        _ = run_builder().print_long_help();
        // return exit status 1 if the commandline was empty, 0 otherwise
        return Ok(commandline.is_empty().into());
    }
    let requested_dir: Option<String> = matches.get_one::<String>("chdir").map(|s| s.into());
    let allowed_mode = check_enabled_or_bail();
    let copy_env = matches.get_flag("copyEnv");

    if !can_current_user_elevate()? {
        // Bail out with an error. main(0) will then print the error message to
        // the user to let them know they aren't allowed to run sudo.
        return Err(ERROR_REQUEST_REFUSED.into());
    }

    let requested_mode = if matches.get_flag("newWindow") {
        Some(SudoMode::ForceNewWindow)
    } else if matches.get_flag("disableInput") {
        Some(SudoMode::DisableInput)
    } else if matches.get_flag("inline") {
        Some(SudoMode::Normal)
    } else {
        None
    };

    log_modes(requested_mode);

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

    run_target(copy_env, &commandline, actual_mode, requested_dir)
}

fn do_elevate(matches: &ArgMatches) -> Result<i32> {
    _ = check_enabled_or_bail();

    let parent_pid = matches.get_one::<String>("PARENT").unwrap().parse::<u32>();
    let nonce = matches.get_one::<String>("NONCE").unwrap().parse::<u32>();

    if let Err(e) = &parent_pid {
        eprintln!("{} {}", r::IDS_UNKNOWNERROR.get(), e);
        std::process::exit(-1);
    }
    trace_log_message(&format!("elevate starting for parent: {parent_pid:?}"));

    trace_log_message(&format!("matches: {matches:?}"));

    let commandline = matches
        .get_many::<String>("COMMANDLINE")
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    let result = start_rpc_server(
        parent_pid.ok().unwrap(),
        nonce.ok().unwrap(),
        None,
        &commandline,
    );
    trace_log_message(&format!("elevate result: {result:?}"));
    result
}

fn do_config(matches: &ArgMatches) -> Result<i32> {
    let mode = match matches.get_one::<String>("enable") {
        Some(mode) => {
            let mode = match mode.as_str() {
                "disable" => SudoMode::Disabled,
                "enable" => SudoMode::Normal,
                "forceNewWindow" => SudoMode::ForceNewWindow,
                "disableInput" => SudoMode::DisableInput,
                "normal" => SudoMode::Normal,
                "default" => SudoMode::Normal,
                _ => {
                    eprintln!("{} {}", r::IDS_INVALIDMODE.get(), mode);
                    std::process::exit(-1);
                }
            };
            try_enable_sudo(mode)?;
            mode
        }
        None => check_enabled_or_bail(),
    };

    match mode {
        SudoMode::Disabled => println!("{}", r::IDS_DISABLEDMESSAGE.get()),
        SudoMode::ForceNewWindow => println!("{}", r::IDS_CURRENTMODE_FORCENEWWINDOW.get()),
        SudoMode::DisableInput => println!("{}", r::IDS_CURRENTMODE_DISABLEINPUT.get()),
        SudoMode::Normal => println!("{}", r::IDS_CURRENTMODE_INLINE.get()),
    }

    Ok(0)
}

fn try_enable_sudo(requested_mode: SudoMode) -> Result<()> {
    let elevated = is_running_elevated()?;
    if !elevated {
        eprintln!("{}", r::IDS_REQUIREADMINTOCONFIG.get());
        std::process::exit(-1);
    }
    let config: RegistryConfigProvider = Default::default();
    let max_mode = get_allowed_mode_from_policy(&config)?;
    if requested_mode > max_mode {
        match max_mode {
            SudoMode::Disabled => eprintln!("{}", r::IDS_DISABLEDBYPOLICY.get()),
            SudoMode::ForceNewWindow => eprintln!("{}", r::IDS_MAXPOLICYMODE_FORCENEWWINDOW.get()),
            SudoMode::DisableInput => eprintln!("{}", r::IDS_MAXPOLICYMODE_DISABLEINPUT.get()),
            SudoMode::Normal => eprintln!("{}", r::IDS_MAXPOLICYMODE_INLINE.get()),
        }
        std::process::exit(-1);
    }

    let result = windows_registry::LOCAL_MACHINE
        .create("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Sudo")
        .and_then(|key| key.set_u32("Enabled", requested_mode.into()));

    if let Err(err) = result {
        eprintln!("{} {}", r::IDS_ERRORSETTINGMODE.get(), err);
        return Err(err);
    }

    Ok(())
}
