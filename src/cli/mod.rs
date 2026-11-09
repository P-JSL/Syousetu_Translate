//! Command routing; reusable parsing/translation services live in library domains.
mod args;
mod command;
use command::Command;
mod commands;
mod factory;
mod help;
mod options;
mod progress;
use crate::translation::session;
use anyhow::Result;
use args::require;
use factory::{local_session, translation_session};
use help::print as help;
pub fn run(args: &[String]) -> Result<()> {
    if args.len() <= 1
        || args
            .iter()
            .skip(1)
            .any(|arg| arg == "--help" || arg == "-h")
        || args.get(1).map(String::as_str) == Some("help")
    {
        help();
        return Ok(());
    }
    options::validate(&args[1], &args[2..])?;
    match args[1].parse::<Command>()? {
        Command::Worker => session::serve(
            options::settings(&args[2..]).and_then(|config| local_session(&args[2..], &config)),
        ),
        Command::Models => commands::settings::model_command(&args[2..]),
        Command::Memory => commands::settings::memory_command(&args[2..]),
        Command::Translate => {
            let config = options::settings(&args[2..])?;
            println!(
                "{}",
                translation_session(&args[2..], &config)?
                    .translate(&require(&args[2..], "--text")?)?
            );
            Ok(())
        }
        Command::InspectConfig => {
            println!(
                "{}",
                serde_json::to_string_pretty(&options::settings(&args[2..])?)?
            );
            Ok(())
        }
        Command::WriteConfig => commands::settings::write_config(&args[2..]),
        Command::Inspect => commands::novel::inspect(&args[2..]),
        #[cfg(feature = "legacy")]
        Command::InspectDat => commands::legacy::inspect_dat(&args[2..]),
        #[cfg(feature = "legacy")]
        Command::InspectRootDll => commands::legacy::inspect_root_dll(&args[2..]),
        #[cfg(feature = "legacy")]
        Command::InspectEhnd => commands::legacy::inspect_ehnd(&args[2..]),
        #[cfg(feature = "legacy")]
        Command::AuditEhndRegex => commands::legacy::audit_ehnd_regex(&args[2..]),
        #[cfg(feature = "legacy")]
        Command::AuditEhndControl => commands::legacy::audit_ehnd_control(&args[2..]),
        #[cfg(feature = "legacy")]
        Command::CompareEngines => commands::legacy::compare_engines(&args[2..]),
        Command::Download => commands::novel::download(&args[2..]),
        Command::File => commands::file::file_translate(&args[2..]),
    }
}
