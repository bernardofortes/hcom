//! `hcom reset` command — archive and clear conversation, optionally hooks/config.
//!
//!
//! Modes:
//!   hcom reset              Clear database (archive conversation)
//!   hcom reset hooks        Remove hooks
//!   hcom reset all          Stop all + clear db + remove hooks + reset config

use crate::db::HcomDb;
use crate::shared::CommandContext;

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetTarget {
    Hooks,
    All,
}

#[derive(clap::Parser, Debug)]
#[command(name = "reset", about = "Reset hcom components")]
pub struct ResetArgs {
    /// Reset scope
    #[arg(value_enum)]
    pub target: Option<ResetTarget>,
}

/// Handle reset modes that do not replace the database files while borrowing
/// the router-owned handle. `Some` means the command completed and the router
/// may still deliver pending messages through that handle. `None` transfers
/// responsibility to [`cmd_reset_destructive`], which consumes the handle.
pub fn cmd_reset_non_destructive(
    db: &HcomDb,
    args: &ResetArgs,
    ctx: Option<&CommandContext>,
    is_inside_ai: bool,
) -> Option<i32> {
    let target = args.target;

    // Confirmation gate: inside AI tools, require --go
    if is_inside_ai && !ctx.map(|c| c.go).unwrap_or(false) {
        super::reset_preview::print_reset_preview(target, db);
        return Some(0);
    }

    // hooks: remove hooks from all locations
    if target == Some(ResetTarget::Hooks) {
        return Some(super::hooks::cmd_hooks_remove(&["all".to_string()]));
    }

    None
}

/// Archive and replace the database after the router transfers its sole
/// in-process handle here. Preview and hooks-only modes must be handled by
/// [`cmd_reset_non_destructive`] before this function is called.
pub fn cmd_reset_destructive(db: HcomDb, args: &ResetArgs, ctx: Option<&CommandContext>) -> i32 {
    let target = args.target;
    let mut exit_codes = Vec::new();

    // Stop all instances before clearing database
    let stop_args = crate::commands::stop::StopArgs {
        targets: vec!["all".into()],
    };
    let stop_exit = crate::commands::stop::cmd_stop(&db, &stop_args, ctx);
    if stop_exit != 0 {
        return stop_exit;
    }
    exit_codes.push(stop_exit);

    // Stop relay daemon if running before clear
    let relay_pid = crate::relay::worker::relay_worker_pid();
    let daemon_exit = crate::commands::daemon::daemon_stop();
    if daemon_exit != 0 {
        return daemon_exit;
    }
    if let Some(pid) = relay_pid {
        for _ in 0..50 {
            if !crate::pidtrack::is_alive(pid) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if crate::pidtrack::is_alive(pid) {
            eprintln!(
                "Error: relay daemon PID {pid} still owns runtime state; database reset was not started"
            );
            return 1;
        }
    }

    // The router transfers its sole in-process database owner into this
    // command. Release it before Windows file lifecycle operations; keeping
    // this handle alive pins hcom.db and its WAL/SHM sidecars on Windows.
    drop(db);

    // Clean temp files
    super::reset_ops::clean_temp_files();

    // Archive and clear database
    let archive_exit =
        super::reset_ops::print_archive_result(super::reset_ops::archive_and_clear_db());
    if archive_exit != 0 {
        return archive_exit;
    }

    // For reset all: clear pidtrack before recovery can trigger
    if target == Some(ResetTarget::All) {
        super::reset_ops::clear_full_reset_artifacts();
    }

    // Log reset event to fresh DB
    super::reset_ops::bootstrap_fresh_db();

    // Respawn relay worker (was stopped above) and push reset event to remote devices.
    // ensure_worker re-reads config, so this is a no-op when relay is not configured.
    crate::relay::worker::ensure_worker(false);
    crate::relay::trigger_push();

    // all: also remove hooks, reset config, clear device identity
    if target == Some(ResetTarget::All) {
        // Remove hooks
        if super::hooks::cmd_hooks_remove(&["all".to_string()]) != 0 {
            exit_codes.push(1);
        }

        // Reset config
        exit_codes.push(super::reset_ops::reset_config());
    }

    exit_codes.into_iter().max().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn reset_args_default_to_db_reset() {
        let args = ResetArgs::try_parse_from(["reset"]).unwrap();
        assert_eq!(args.target, None);
    }

    #[test]
    fn reset_args_parse_named_target() {
        let args = ResetArgs::try_parse_from(["reset", "hooks"]).unwrap();
        assert_eq!(args.target, Some(ResetTarget::Hooks));
    }
}
