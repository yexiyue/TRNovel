cargo release changes
cargo release version patch --execute
cargo release commit --execute
cargo release publish --registry crates-io --execute
cargo release tag --execute
cargo release push --execute