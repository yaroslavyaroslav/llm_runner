# AGENTS.md

Project-specific notes for working on the Rust core in this repo.

## Scope

- This repo is the provider/runtime core.
- It is paired with a separate Sublime Text wrapper repo named `OpenAI completion`.
- Changes here should preserve a thin wrapper model: provider-specific behavior belongs in Rust, not in the Sublime-side frontend glue.

## Provider Architecture

- The core supports four API tracks:
  - OpenAI Responses
  - Anthropic Messages
  - Google Gemini native API
  - legacy OpenAI-compatible `/chat/completions`
- The provider-independent conversation layer lives in `src/provider.rs`.
- Network transport and streaming normalization live in `src/network_client.rs`.
- The runner consumes unified assistant/tool results in `src/runner.rs`.
- `ApiType` supports:
  - `open_ai`
  - `open_ai_responses`
  - `anthropic`
  - `google`
  - backward-compatible alias: `antropic`

## Tool Calling

- Multiple tool calls in a single assistant turn are supported across all four provider tracks.
- Local tool execution in the runner is still serial.
- "Parallel tool calls" currently means preserving and replaying multiple calls in one model turn, not true concurrent local execution.

## Provider-Specific Rules

### OpenAI Responses

- This is the preferred OpenAI-native path.
- New OpenAI work should target Responses first, not legacy chat/completions.

### Legacy `/chat/completions`

- Keep tolerant JSON recovery only here.
- This path exists for Together, OpenRouter, Grok-style, and other OpenAI-compatible providers that may emit malformed or fragmented stream payloads.
- Do not contaminate native provider code paths with compatibility hacks intended only for legacy providers.

### Anthropic

- Anthropic model ids can go stale. Do not assume aliases remain valid.
- During this session, `claude-haiku-4-5-20251001` was the working model and older defaults like `claude-3-5-haiku-latest` were stale for the tested org.

### Google Gemini

- Tool schema must be normalized before sending to Gemini.
- Fields like `additionalProperties`, `default`, `$ref`, `$defs`, and `format` cannot be forwarded blindly.
- Gemini-specific part metadata like `thoughtSignature` must be preserved at the part level.

## Environment Conventions

Use these env var names consistently:

- `OPENAI_API_KEY`
- `ANTHROPIC_API_KEY`
- `GEMINI_API_KEY`
- `TOGETHER_API_KEY`
- `PROXY`

Avoid reintroducing older names:

- `OPENAI_API_TOKEN`
- `ANTROPIC_API_KEY`
- `GOOGLE_API_KEY`
- `TOGETHER_API_TOKEN`

## Testing Notes

- Remote ignored tests exist in `tests/worker_test.rs` for:
  - OpenAI Responses
  - Anthropic
  - Google Gemini
  - Together chat/completions
- `PROXY` can affect results and previously caused false OpenAI connection failures until handled explicitly.
- During this session, remote suites were confirmed green for OpenAI Responses, Anthropic, Google, and Together legacy chat/completions.
- Command-level test pipelines, interpreter setup, and operator workflows belong in reusable skills or private operator docs, not in this public repo note.

## Python / Build Notes

- Python 3.14 auto-detection can break local PyO3 runs if the interpreter is not pinned intentionally.
- `maturin` was used during the session, but installation/setup details should live in skills or private setup docs rather than here.

## CI Lessons

- CI workflow file: `.github/workflows/CI.yml`
- The working wheel matrix is intentionally pinned, not floating:
  - smoke/bootstrap Python for `linux`: `3.8`
  - smoke/bootstrap Python for `musllinux`: `3.8`
  - smoke/bootstrap Python for `macos`: `3.8`
  - wheel build matrix for `windows`: `3.8`, `3.13`
- Stable wheel targets kept in CI:
  - `linux x86_64`
  - `musllinux x86_64`
  - `musllinux aarch64`
  - `macos x86_64`
  - `macos aarch64`
  - `windows x64`
  - `windows x86`
- Removed problematic targets:
  - `linux gnu aarch64`
  - `linux armv7`
  - `musllinux x86`
  - `musllinux armv7`
- The main stability fix for Linux/musl was switching `reqwest` to `rustls-tls` and removing the OpenSSL dependency path.
- For GitHub Actions on Windows, do not use `pip install dist/*.whl` in a `pwsh` step.
  PowerShell will pass the literal wildcard, so smoke tests can fail with a fake `ModuleNotFoundError` simply because the wheel was never installed.
- On Unix smoke tests, do not install the "first wheel in dist".
  Select the wheel matching the current interpreter tag (`cp38`, `cp313`, etc.), or the smoke test may accidentally try an incompatible wheel and produce a misleading platform error.
- Avoid `actions/setup-python` with `python-version: 3.x` for wheel smoke tests.
  Hosted runners may move to unsupported interpreters like `3.14`, which makes PyO3/maturin failures look like packaging regressions when they are really runner drift.
- Windows wheel smoke tests should import the built extension directly after installation.
  The current known-good check is `python -c "import llm_runner; print(llm_runner.__file__)"`.

## Windows Packaging Notes

- The bad Windows regression in `0.2.13` was the `python3.dll` linkage.
- The fixed Windows `0.2.14` `cp38 win_amd64` wheel links back to `python38.dll`, matching `0.2.12`.
- Comparing `0.2.12` and `0.2.14` win_amd64 wheels:
  - both depend on `python38.dll`
  - both depend on `VCRUNTIME140.dll`
  - `0.2.14` is larger and has a different import set, but not the old `python3.dll` mistake
- Action artifacts and the PyPI-published `0.2.14` wheel were not byte-identical, but they matched on layout, tags, and critical DLL imports.

## Sublime / Package Control Lessons

- When a Sublime user reports a Windows import failure, first verify the actually installed library version from the Sublime log before assuming the latest PyPI wheel is in use.
- During this session, a user report still showed:
  - `Installed library "llm_runner" 0.2.13 for Python 3.8`
  even though `0.2.14` was already present on PyPI.
- `openai-sublime-text` currently depends on `llm_runner` without a version pin in `dependencies.json`, so if users still receive an older release, suspect Package Control indexing/caching before suspecting the newest wheel.

## Reference Repo

- The `openfang` repo was useful as a design reference, not as a dependency to import wholesale.
- Most useful ideas taken from that review:
  - provider-neutral stream/event model
  - provider quirks as explicit rules
  - schema normalization per provider
  - preserving Google part-level metadata
- Do not try to pull in the whole `openfang` crate graph just for reuse.

## Wrapper Coordination

- The paired wrapper repo `OpenAI completion` received a `6.0.0` release during this session.
- Its deprecated "finite state / no more connectors" note was removed from the wrapper `README.md`.
- Release notes for that wrapper release were added under its `messages/6.0.0.md`.
- Keep this repo focused on runtime/provider behavior and keep wrapper logic thin.

## Recommended Starting Points

If continuing provider work, start here:

1. `src/provider.rs`
2. `src/network_client.rs`
3. `tests/worker_test.rs`
4. `tests/test_worker_python.py`

If debugging build or release issues, start here:

1. `.github/workflows/CI.yml`
2. `Cargo.toml`
3. `Cargo.lock`

## Rule Of Thumb

- Native provider APIs should stay clean and strict.
- Ugly defensive parsing belongs only in legacy OpenAI-compatible chat/completions.
- If a provider behaves "almost OpenAI" but not quite, test and handle it through the legacy compatibility path instead of polluting native-provider logic.
