# Examples

This directory contains examples of how to use GTFS Guru.

## Python Demo

`python_demo.ipynb` demonstrates how to interact with GTFS Guru using its Python bindings. You can use it to validate GTFS feeds programmatically and analyze the results within a Python environment.

### Prerequisites

To run this demo, you need to install the `gtfs-guru` Python package. Since this package is backed by high-performance Rust code, you need to build it from the source initially.

**Requirements:**

* Python 3.8+
* Rust toolchain (install via [rustup.rs](https://rustup.rs/))
* `maturin` (build tool for Rust/Python)
* `jupyterlab` or `notebook` (to run the .ipynb file)

### Setup & Installation

1. **Install build tools and Jupyter:**

    ```bash
    pip install maturin jupyterlab
    ```

2. **Build and install the package locally:**
    Run this command from the root of the repository:

    ```bash
    maturin develop -m crates/gtfs_validator_python/Cargo.toml --release
    ```

    *Note: The `--release` flag ensures the validator runs at maximum speed.*

### Running the Demo

1. Navigate to the examples directory:

    ```bash
    cd examples
    ```

2. Start Jupyter:

    ```bash
    jupyter lab
    ```

3. Open `python_demo.ipynb` and run the cells.
