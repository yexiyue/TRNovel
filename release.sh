cargo release changes
cargo release version --workspace patch --execute
cargo release commit --execute
cargo release publish --workspace --registry crates-io --execute
cargo release tag --workspace --execute
cargo release push --workspace --execute