alias r := run
alias c := check
alias t := test
alias p := psql

_default:
    @just --list

run *args:
    cargo run --locked {{args}}

check *args:
    cargo clippy --locked {{args}}

test *args:
    cargo test --locked {{args}}

psql *args:
    psql "$DATABASE_URL" {{args}}
