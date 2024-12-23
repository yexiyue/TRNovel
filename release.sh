cargo fmt --all --check
cargo clippy --all-targets --all-features --workspace -- -D warnings
cargo release changes
cargo release version minor --execute --no-confirm
cargo release commit --execute --no-confirm
cargo release publish --registry crates-io --execute --no-confirm
cargo release tag --execute --no-confirm
cargo release push --execute --no-confirm