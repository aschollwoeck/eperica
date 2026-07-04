//! `eperica-bots` binary entry point.
//!
//! Full flag/env parsing (`--server`, `--world`, `--keys`, `--dry-run`,
//! `--tick-secs`, `--cap`) and the fleet loop are added in T4.  This stub
//! ensures the crate compiles as both lib and bin from T1 onwards.

fn main() {
    eprintln!(
        "Usage: eperica-bots --server <url> --world <id> --keys <path> \
         [--dry-run] [--tick-secs <n>] [--cap <n>]"
    );
}
