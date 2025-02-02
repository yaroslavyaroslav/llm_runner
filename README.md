# Sublime LLM Communication Core

This project serves as a core engine for communicating with various Large Language Model (LLM) backends, with a primary focus on integration with Sublime Text. Currently, network communication is implemented for the OpenAI gateway, with plans to support OpenAI-alike models and Claude.

Core of [OpenAI-sublime-text package](https://github.com/yaroslavyaroslav/OpenAI-sublime-text).

## Overview

- **Functionality**: This core is designed to interface with LLMs, manage cache entries, handle network requests, and process streaming data.
- **Integrations**: Primarily targeted at Sublime Text, with support for Python bindings via PyO3 to facilitate usage in Python environments.


## Getting Started

### Prerequisites

- **Rust**: Ensure `cargo` and `rustc` are installed.

### Installation

1. Clone the repository:
   ```bash
   git clone git@github.com:yaroslavyaroslav/llm_runner.git
   cd llm_runner
   ```

2. Build the project:

> [!NOTE]
> `python 3.8`

   ```bash
   maturin build --release
   ```

3. Install the it as dependency for ST (==under python 3.8 venv==):
> [!NOTE]
> `python 3.8`
   ```bash
   pip install target/wheels/llm_runner-0.2.0-cp38-cp38-macosx_11_0_arm64.whl --target '/path/to/Sublime Text/Lib/python38/' --upgrade
   ```

### Usage

- Integrate with Sublime Text using the provided Python classes or directly using Rust API.
- Configure and use in your own extensions or projects that require LLM interaction.

### Configuration

- **Assistant Settings**: Modify settings in `AssistantSettings` struct for your specific LLM configurations and preferences.
- **Cache Handling**: Manage cache directory and file paths as per your application's needs.

## Development

All contributions are welcome! To get started with development:

1. Fork the repository.
2. Make your changes in a new branch.
3. Create a pull request when ready.

### Testing

Run tests with:
```bash
cargo test
```

Utilize the extensive test suite included for validation of different components, including network client and cache handling.

## Future Plans

- Additional LLM backend support.
- Enhance cache management.
- Expand Python bindings for broader application compatibility.

## License

This project is licensed under custom license. See the `LICENSE` file for more details.