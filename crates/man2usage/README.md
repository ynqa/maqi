# man2usage

Generate Usage KDL from installed or Debian command manuals.

`man2usage` is a standalone Rust CLI. It acquires manuals, uses `mandoc` to
interpret man/mdoc source, and extracts command information from the rendered
HTML with `scraper`. The resulting KDL is validated by `usage-lib`.

## Usage

Choose exactly one input source:

```sh
# Look up installed manuals. Missing manuals are an error.
cargo run -p man2usage -- --bin ls --output ls.usage.kdl

# Start from the root command's Debian man-page URL.
cargo run -p man2usage -- \
  --debian-man-url https://manpages.debian.org/unstable/kubernetes-client/kubectl.1.en.html \
  --output kubectl.usage.kdl
```

Omit `--output` to write KDL to stdout. Progress and extraction diagnostics go to
stderr. Generation completes before the output file is written, so acquisition
or parsing failures leave existing output untouched.

The `mandoc` executable must be available on `PATH`. The `--bin` input also
requires `man`. There are no format switches, fetch/generate subcommands, local
input paths, or URL templates.

## Input sources

`--bin COMMAND` invokes `man -w COMMAND` using the local manpath configuration.
It fails immediately if the manual is missing, even if the executable exists.
It follows references to sibling `<command>-*` manuals in sections 1 and 8 from
the same directory. A referenced but missing sibling is an error. Gzip sources
are supported; uncompressed copies take precedence. It never executes the
command or falls back to a download.

`--debian-man-url URL` accepts an HTTPS HTML man-page link on
`manpages.debian.org`. Abbreviated links are resolved through Debian's redirect.
Use the root command's page, not an individual subcommand or a package index.
The loader reads that package's index and downloads the root and all
`<command>-*` command manuals, including nested subcommands, from the same
suite, package, and language. Section 1 and 8 pages are considered. There is no
need to list child URLs or enable a follow option. Any failed download aborts
generation; it does not silently emit a partial command tree.

Downloads remain in memory. Temporary files used by mandoc are removed after
parsing. No manuals are installed or retained. URLs such as `unstable` can
change, and the fetched documentation may differ from the installed binary.
Sources are limited to 8 MiB each and 1,000 pages per invocation; individual
HTTP requests have a 40-second timeout.

## Extraction

The common pipeline is:

```text
acquire raw manuals -> mandoc HTML -> command model -> Usage KDL
```

The parser uses NAME and SYNOPSIS for command paths and descriptions, and
option lists in OPTIONS and DESCRIPTION for flags. It accepts tagged lists,
subsection headings, and paragraphs documenting `option=default` followed by a
tab and description. These are document structures, not framework-specific
parser modes. Explicit boolean defaults identify switches; `[]` defaults
identify repeated values. Explicit finite choices are retained.

Inherited options are read where documented. An option is emitted as global
only when all descendant manuals explicitly document the same inherited
option; otherwise inherited flags are emitted on the commands that document
them. Every command must have its parent manual. A filename alone does not
establish a subcommand path.

Optional values, grouped flags, and ambiguous declarations are skipped with a
diagnostic. Positional arguments remain generic optional `[args]...`; required
positionals and cross-option constraints are not inferred. This produces static
completion data, not a complete execution-validation schema. Roff includes
must be resolved explicitly. Mandoc errors abort generation.

## Development

- `input/bin.rs`: installed-manual discovery.
- `input/debian.rs`: Debian page and package-index acquisition.
- `parse.rs`: mandoc invocation and common HTML extraction.
- `model.rs`: extracted command information.
- `emit.rs`: KDL output and Usage validation.

```sh
cargo test -p man2usage --locked
cargo clippy -p man2usage --locked --all-targets -- -D warnings
cargo fmt -p man2usage --check
```

Parser tests cover man/mdoc fixtures, option defaults, and inherited options.
Debian discovery tests use in-memory HTTP responses and exercise package,
suite, language, and descendant selection without the public service. CLI
smoke tests cover input exclusivity, installed-manual lookup, output, and
failures. Tests require `mandoc` and `man` but do not require external network.
