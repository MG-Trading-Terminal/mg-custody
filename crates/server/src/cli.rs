//! Admin CLI for operational tasks.
//!
//! Usage:
//!   custody-admin rescan --chain ethereum --from-block 20000000
//!   custody-admin freeze --reason "Suspicious activity"
//!   custody-admin unfreeze
//!   custody-admin sweep --chain ethereum --from hot --to warm
//!   custody-admin status
//!   custody-admin keygen --scheme frost --threshold 2 --parties 3

use std::collections::HashMap;

/// CLI command.
#[derive(Debug)]
pub enum AdminCommand {
    /// Rescan blocks from a specific height.
    Rescan { chain: String, from_block: u64 },
    /// Emergency freeze all operations.
    Freeze { reason: String },
    /// Lift emergency freeze (requires admin auth).
    Unfreeze,
    /// Trigger a manual sweep.
    Sweep { chain: String, from_tier: String, to_tier: String },
    /// Show system status.
    Status,
    /// Run a key generation ceremony.
    Keygen { scheme: String, threshold: u16, parties: u16 },
    /// Show wallet balances.
    Balances,
    /// List pending approvals.
    PendingApprovals,
}

/// Parse CLI arguments into a command.
pub fn parse_args(args: &[String]) -> Option<AdminCommand> {
    if args.len() < 2 {
        print_usage();
        return None;
    }

    match args[1].as_str() {
        "rescan" => {
            let chain = get_flag(args, "--chain").unwrap_or("ethereum".into());
            let from_block: u64 = get_flag(args, "--from-block")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            Some(AdminCommand::Rescan { chain, from_block })
        }
        "freeze" => {
            let reason = get_flag(args, "--reason").unwrap_or("Manual freeze".into());
            Some(AdminCommand::Freeze { reason })
        }
        "unfreeze" => Some(AdminCommand::Unfreeze),
        "sweep" => {
            let chain = get_flag(args, "--chain").unwrap_or("ethereum".into());
            let from = get_flag(args, "--from").unwrap_or("hot".into());
            let to = get_flag(args, "--to").unwrap_or("warm".into());
            Some(AdminCommand::Sweep { chain, from_tier: from, to_tier: to })
        }
        "status" => Some(AdminCommand::Status),
        "keygen" => {
            let scheme = get_flag(args, "--scheme").unwrap_or("frost".into());
            let threshold: u16 = get_flag(args, "--threshold").and_then(|s| s.parse().ok()).unwrap_or(2);
            let parties: u16 = get_flag(args, "--parties").and_then(|s| s.parse().ok()).unwrap_or(3);
            Some(AdminCommand::Keygen { scheme, threshold, parties })
        }
        "balances" => Some(AdminCommand::Balances),
        "approvals" => Some(AdminCommand::PendingApprovals),
        _ => {
            print_usage();
            None
        }
    }
}

fn get_flag(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

fn print_usage() {
    eprintln!("Usage: custody-admin <command> [options]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  rescan     --chain <chain> --from-block <n>     Rescan blocks");
    eprintln!("  freeze     --reason <reason>                     Emergency freeze");
    eprintln!("  unfreeze                                         Lift freeze");
    eprintln!("  sweep      --chain <chain> --from <tier> --to <tier>  Manual sweep");
    eprintln!("  status                                           System status");
    eprintln!("  keygen     --scheme <frost|ecdsa> --threshold <n> --parties <n>");
    eprintln!("  balances                                         Wallet balances");
    eprintln!("  approvals                                        Pending approvals");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parse_rescan() {
        let cmd = parse_args(&args("custody-admin rescan --chain ethereum --from-block 20000000")).unwrap();
        match cmd {
            AdminCommand::Rescan { chain, from_block } => {
                assert_eq!(chain, "ethereum");
                assert_eq!(from_block, 20000000);
            }
            _ => panic!("Wrong command"),
        }
    }

    #[test]
    fn parse_freeze() {
        let cmd = parse_args(&args("custody-admin freeze --reason suspicious")).unwrap();
        match cmd {
            AdminCommand::Freeze { reason } => assert_eq!(reason, "suspicious"),
            _ => panic!("Wrong command"),
        }
    }

    #[test]
    fn parse_keygen() {
        let cmd = parse_args(&args("custody-admin keygen --scheme frost --threshold 2 --parties 3")).unwrap();
        match cmd {
            AdminCommand::Keygen { scheme, threshold, parties } => {
                assert_eq!(scheme, "frost");
                assert_eq!(threshold, 2);
                assert_eq!(parties, 3);
            }
            _ => panic!("Wrong command"),
        }
    }

    #[test]
    fn parse_status() {
        let cmd = parse_args(&args("custody-admin status")).unwrap();
        assert!(matches!(cmd, AdminCommand::Status));
    }

    #[test]
    fn empty_args_returns_none() {
        assert!(parse_args(&args("custody-admin")).is_none());
    }
}
