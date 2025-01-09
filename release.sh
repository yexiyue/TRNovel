cargo fmt --all --check
cargo clippy --all-targets --all-features --workspace -- -D warnings
cargo release changes
cargo release version patch --execute --no-confirm --workspace
cargo release commit --execute --no-confirm
cargo release publish --registry crates-io --execute --no-confirm --workspace
cargo release tag --execute --no-confirm --workspace
cargo release push --execute --no-confirm --workspace

# 没有workspace的情况下
# cargo fmt --all --check
# cargo clippy --all-targets --all-features -- -D warnings
# cargo release changes
# cargo release version patch --execute --no-confirm
# cargo release commit --execute --no-confirm
# cargo release publish --registry crates-io --execute --no-confirm
# cargo release tag --execute --no-confirm
# cargo release push --execute --no-confirm