# Contributing

Thanks for helping improve MarkDraft.

## Licensing of contributions

MarkDraft is source-available: free for noncommercial use under the
[PolyForm Noncommercial License](LICENSE.md) and sold under a
[commercial license](COMMERCIAL.md) to businesses. So that contributions
can be included in both, every pull request must include this statement in
its description:

> I license my contribution to the MarkDraft maintainers under the
> PolyForm Noncommercial License 1.0.0, and I grant the maintainers a
> perpetual, irrevocable, worldwide, royalty-free right to also license it
> under any other terms, including commercial ones.

Pull requests without it cannot be merged.

## Before you open a pull request

```sh
pnpm typecheck && pnpm test
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Record design decisions in [DECISIONS.md](DECISIONS.md) and keep the IPC
table in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) current.
