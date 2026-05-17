# Build, Test, And Documentation

## Rust Build

From the workspace root:

```bash
cargo build
```

Release build:

```bash
cargo build --release
```

## Rust Tests

Run the workspace tests:

```bash
cargo test --workspace
```

Focused examples:

```bash
cargo test -p protocol
cargo test -p session-kernel
cargo test -p status
```

## VSCode Extension Check

From `apps/vscode-extension`:

```bash
npm run check
```

The check script runs:

```bash
node --check extension.js
```

## Documentation Source

The documentation source is in:

```text
docs/src
```

The mdBook config is:

```text
docs/book.toml
```

## Build Documentation

The local mdBook install used for this repo lives under `target/mdbook-bin`.

```bash
target/mdbook-bin/bin/mdbook build docs
```

Output:

```text
docs/site
```

Open:

```text
docs/site/index.html
```

## Documentation Quality Checks

Before publishing, run:

```bash
target/mdbook-bin/bin/mdbook build docs
cargo test --workspace
npm --prefix apps/vscode-extension run check
```

Review at least:

- landing page
- architecture page
- each crate page
- interface reference
- generated search index

## Publication Checklist

- Every workspace crate has a design page.
- Every runtime entrypoint is documented.
- Every public cross-crate trait is documented.
- JSON process messages are documented.
- Runtime flows are documented.
- Data locations and build commands are documented.
- Generated HTML builds without mdBook warnings.
