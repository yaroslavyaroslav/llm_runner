# Sublime LLM Communication Core

This project serves as a core engine for communicating with various Large Language Model (LLM) backends, with a primary focus on integration with Sublime Text. Currently, network communication is implemented for the OpenAI gateway, with plans to support OpenAI-alike models and Claude.

Core of [OpenAI-sublime-text package](https://github.com/yaroslavyaroslav/OpenAI-sublime-text).

## Overview

- **Functionality**: This core is designed to interface with LLMs, manage cache entries, handle network requests, and process streaming data.
- **Integrations**: Primarily targeted at Sublime Text, with support for Python bindings via PyO3 to facilitate usage in Python environments.

## Key Modules

### Types

Defines core data structures used across the project, including enums, structs for cache management, input processing, and assistant settings. These are serialized/deserialized using `serde` and interfaced with Python using `pyo3`.

### Runner

Implements the logic to send requests to LLMs and handle responses. It interfaces with network clients and manages cache data for requests.

### Worker

A bridge between the frontend (e.g., Sublime Text) and the core functionality, handling request execution and response streaming. Supports cancellations and tracks the status of ongoing operations.

### Network Client

Handles HTTP requests to LLM backends using `reqwest`. Manages headers, payload preparation, and response handling, with specific support for streaming data via SSE.

### Cache

Manages the storage and retrieval of request/response history and model-specific data to/from the file system. Ensures efficient access and persistence of session data.

### Stream Handler

Processes streamed responses from LLMs, invoking user-defined callback functions to handle live data updates.

### Python Worker

Provides a Python interface for the core functionality, allowing interaction using Python scripts via the `pyo3` runtime.

### OpenAI Network Types

Defines specific request and response structures for OpenAI communication, including message formatting and response deserialization.

## Getting Started

### Prerequisites

- **Rust**: Ensure `cargo` and `rustc` are installed.
- **Python**: Python environment for interfacing via PyO3.
- **Tokio**: For asynchronous operations.
- **Serde**: For JSON serialization/deserialization.
- **Reqwest**: For HTTP requests.

### Installation

1. Clone the repository:
   ```bash
   git clone git@github.com:yaroslavyaroslav/ai_helper.git
   cd ai_helper
   ```

2. Build the project:

> [!NOTE]
> `python 3.8`

   ```bash
   maturin build
   ```

3. Install the it as dependency for ST (::under python 3.8 venv::):
> [!NOTE]
> `python 3.8`
   ```bash
   pip install target/wheels/llm_runner-0.1.0-cp38-cp38-macosx_11_0_arm64.whl --target '/path/to/Sublime Text/Lib/python38/' --upgrade
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