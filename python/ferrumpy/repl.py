"""
FerrumPy REPL Session Manager

Manages the lifecycle of an evcxr REPL session with variables
captured from an LLDB debugging session.
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Dict, Optional

# NOTE: OutputDrainer is disabled because:
# 1. evcxr now has background threads that drain stdout/stderr internally
# 2. In prompt_toolkit environments, background thread print() calls conflict with UI
# 3. The draining happens automatically via evcxr's internal channels
#
# class OutputDrainer(threading.Thread):
#     """
#     Background thread that continuously drains subprocess output.
#
#     This prevents stdout/stderr pipes from filling up and blocking the subprocess
#     when background threads or async code produce output.
#     """
#     def __init__(self, session, output_callback=None):
#         super().__init__(daemon=True)  # Daemon thread exits with main program
#         self.session = session
#         self.output_callback = output_callback or print
#         self.running = True
#         self.poll_interval = 0.02  # 20ms polling interval
#
#     def run(self):
#         """Main loop: continuously drain output."""
#         while self.running:
#             try:
#                 # Drain stdout
#                 stdout_lines = self.session.drain_stdout()
#                 for line in stdout_lines:
#                     print(line, flush=True)
#
#                 # Drain stderr
#                 stderr_lines = self.session.drain_stderr()
#                 for line in stderr_lines:
#                     print(line, file=sys.stderr, flush=True)
#
#             except Exception as e:
#                 # Ignore errors during draining (e.g., session closed)
#                 # But print to debug if needed
#                 # print(f"[Drainer error: {e}]", file=sys.stderr)
#                 pass
#
#             # Sleep to avoid busy-waiting
#             time.sleep(self.poll_interval)
#
#     def stop(self):
#         """Stop the draining thread."""
#         self.running = False


try:
    import lldb
except ImportError:
    lldb = None

from .serializer import serialize_frame


class ReplSession:
    """Manages an evcxr REPL session with captured debug state."""

    def __init__(self, project_path: str, frame=None):
        """
        Initialize a REPL session.

        Args:
            project_path: Path to the user's Rust project
            frame: LLDB SBFrame to capture variables from
        """
        self.project_path = Path(project_path)
        self.frame = frame
        self.temp_dir: Optional[Path] = None
        self.process: Optional[subprocess.Popen] = None
        self.serialized_data: Dict[str, Any] = {}

    def prepare(self) -> str:
        """
        Prepare the REPL environment.

        Returns:
            Path to the generated REPL project
        """
        # Create temp directory
        self.temp_dir = Path(tempfile.mkdtemp(prefix="ferrumpy_repl_"))

        # 1. Generate lib crate from user project using libgen
        lib_path = self._generate_lib()

        # 2. Serialize frame variables
        if self.frame:
            self.serialized_data = serialize_frame(self.frame)

        # 3. Create REPL project that depends on generated lib
        self._create_repl_project(lib_path)

        # 4. Write serialized data
        data_path = self.temp_dir / "data.json"
        with open(data_path, 'w') as f:
            json.dump(self.serialized_data, f, indent=2)

        # 5. Generate init code
        init_code = self.generate_init_code()
        init_file = self.temp_dir / "init.evcxr"
        init_file.write_text(init_code)

        return str(self.temp_dir)

    def _generate_lib(self) -> Path:
        """Generate lib crate from user project."""
        lib_dir = self.temp_dir / "generated_lib"
        lib_dir.mkdir()

        # Try to use ferrumpy-core libgen if available
        try:
            # For now, just copy user source and transform manually
            self._simple_lib_generation(lib_dir)
        except Exception as e:
            print(f"Warning: libgen failed: {e}")
            # Create minimal lib
            self._create_minimal_lib(lib_dir)

        return lib_dir

    def _simple_lib_generation(self, lib_dir: Path):
        """Simple lib generation by copying and transforming source."""
        src_dir = lib_dir / "src"
        src_dir.mkdir()

        # Find source file
        user_main = self.project_path / "src" / "main.rs"
        user_lib = self.project_path / "src" / "lib.rs"

        if user_lib.exists():
            # User has lib.rs, just copy it
            shutil.copy(user_lib, src_dir / "lib.rs")
        elif user_main.exists():
            # Transform main.rs to lib.rs
            content = user_main.read_text()
            lib_content = self._transform_to_lib(content)
            (src_dir / "lib.rs").write_text(lib_content)
        else:
            raise FileNotFoundError("No main.rs or lib.rs found")

        # Generate Cargo.toml
        self._generate_lib_cargo_toml(lib_dir)

    def _transform_to_lib(self, content: str) -> str:
        """Transform main.rs content to lib.rs format."""
        import re

        lines = content.split('\n')
        result = []
        in_main = False
        brace_count = 0

        # Add serde import
        result.append("use serde::{Serialize, Deserialize};")
        result.append("")

        for line in lines:
            # Skip fn main
            if re.match(r'\s*fn\s+main\s*\(', line):
                in_main = True
                brace_count = line.count('{') - line.count('}')
                continue

            if in_main:
                brace_count += line.count('{') - line.count('}')
                if brace_count <= 0:
                    in_main = False
                continue

            # Make structs/enums public and add serde derive
            if re.match(r'\s*struct\s+\w+', line) or re.match(r'\s*enum\s+\w+', line):
                # Add derive if not present
                if not any('#[derive' in prev for prev in result[-3:]):
                    result.append('#[derive(Debug, Clone, Serialize, Deserialize)]')
                # Make public
                if not line.strip().startswith('pub'):
                    line = 'pub ' + line.lstrip()

            # Make functions public
            if re.match(r'\s*fn\s+\w+', line) and not line.strip().startswith('pub'):
                line = 'pub ' + line.lstrip()

            result.append(line)

        return '\n'.join(result)

    def _generate_lib_cargo_toml(self, lib_dir: Path):
        """Generate Cargo.toml for the lib crate."""
        user_cargo = self.project_path / "Cargo.toml"

        cargo_content = """[package]
name = "ferrumpy_snapshot_lib"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["rlib"]

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
"""

        # Copy user dependencies if available
        if user_cargo.exists():
            try:
                import re
                content = user_cargo.read_text()
                deps_match = re.search(r'\[dependencies\](.*?)(?=\[|\Z)', content, re.DOTALL)
                if deps_match:
                    deps = deps_match.group(1).strip()
                    # Filter out existing serde
                    dep_lines = [dep_line for dep_line in deps.split('\n')
                                if dep_line.strip() and not dep_line.startswith('serde')]
                    if dep_lines:
                        cargo_content += '\n'.join(dep_lines) + '\n'
            except Exception:
                pass

        (lib_dir / "Cargo.toml").write_text(cargo_content)

    def _create_minimal_lib(self, lib_dir: Path):
        """Create a minimal lib when libgen fails."""
        src_dir = lib_dir / "src"
        src_dir.mkdir(exist_ok=True)

        (src_dir / "lib.rs").write_text("""
// Minimal lib - libgen failed
pub use std::*;
""")

        (lib_dir / "Cargo.toml").write_text("""[package]
name = "ferrumpy_snapshot_lib"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["rlib"]

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
""")

    def _create_repl_project(self, lib_path: Path):
        """Create the REPL Cargo project."""
        # This project depends on the generated lib
        cargo_content = f"""[package]
name = "ferrumpy_repl"
version = "0.1.0"
edition = "2021"

[dependencies]
ferrumpy_snapshot_lib = {{ path = "{lib_path}" }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
"""
        (self.temp_dir / "Cargo.toml").write_text(cargo_content)

        # Create src directory
        src_dir = self.temp_dir / "src"
        src_dir.mkdir(exist_ok=True)
        (src_dir / "lib.rs").write_text("// REPL placeholder\n")

    def generate_init_code(self) -> str:
        """Generate evcxr initialization code."""
        lines = []

        # Add dependencies
        lines.append(':dep serde = { version = "1", features = ["derive"] }')
        lines.append(':dep serde_json = "1"')
        lines.append(f':dep ferrumpy_snapshot_lib = {{ path = "{self.temp_dir}/generated_lib" }}')
        lines.append('')

        # Import everything from the lib
        lines.append('use ferrumpy_snapshot_lib::*;')
        lines.append('use serde::{Serialize, Deserialize};')
        lines.append('')

        # Deserialize variables
        if self.serialized_data.get('variables'):
            for name, value in self.serialized_data['variables'].items():
                type_hint = self.serialized_data.get('types', {}).get(name, 'auto')
                json_str = json.dumps(value)

                # Generate let binding
                lines.append(f'// {name}: {type_hint}')
                lines.append(f'let {name} = serde_json::from_str::<serde_json::Value>(r#"{json_str}"#).unwrap();')

        lines.append('')
        lines.append('// Variables are ready! Try: user, config, numbers, etc.')

        return '\n'.join(lines)

    def start_repl(self) -> bool:
        """
        Start the evcxr REPL.

        Returns:
            True if REPL started successfully
        """
        if not self.temp_dir:
            self.prepare()

        # Check if evcxr is available
        try:
            subprocess.run(['evcxr', '--version'], capture_output=True, check=True)
        except (subprocess.CalledProcessError, FileNotFoundError):
            print("Error: evcxr not found. Install with: cargo install evcxr_repl")
            return False

        # Generate init code
        init_code = self.generate_init_code()
        init_file = self.temp_dir / "init.evcxr"
        init_file.write_text(init_code)

        print(f"REPL project: {self.temp_dir}")
        print(f"Init file: {init_file}")
        print("\nTo start manually:")
        print(f"  cd {self.temp_dir}")
        print("  evcxr")
        print(f"  # Then paste contents of {init_file}")

        # Start evcxr (interactive mode requires PTY, simplified here)
        try:
            os.chdir(self.temp_dir)
            os.system('evcxr')
            return True
        except Exception as e:
            print(f"Error starting REPL: {e}")
            return False

    def cleanup(self):
        """Clean up temporary files."""
        if self.temp_dir and self.temp_dir.exists():
            shutil.rmtree(self.temp_dir, ignore_errors=True)

    def __enter__(self):
        self.prepare()
        return self

    def __exit__(self, *args):
        self.cleanup()


def start_repl_from_frame(project_path: str, frame) -> bool:
    """
    Convenience function to start a REPL from an LLDB frame.

    Args:
        project_path: Path to the user's Rust project
        frame: LLDB SBFrame

    Returns:
        True if REPL started successfully
    """
    session = ReplSession(project_path, frame)
    return session.start_repl()


class EmbeddedReplSession:
    """
    Embedded REPL session using Rust-backed evcxr integration.

    This session runs evcxr directly within ferrumpy-core.so,
    without needing a separate evcxr installation.
    """

    def __init__(self, frame=None, project_path: str = None):
        """
        Initialize an embedded REPL session.

        Args:
            frame: LLDB SBFrame to capture variables from
            project_path: Path to user's Rust project (auto-detected if None)
        """
        self.frame = frame
        self.project_path = project_path
        self._session = None
        self._initialized = False
        self._lib_path = None
        self._lib_name = None
        self._drainer = None  # Background thread for output draining

    def _get_rust_session(self):
        """Get or create the Rust ReplSession."""
        if self._session is None:
            try:
                from .ferrumpy_core import PyReplSession
                self._session = PyReplSession()
            except ImportError as ie:
                raise RuntimeError(
                    f"ferrumpy_core not available: {ie}. "
                    "Build with: maturin develop --features python"
                )
            except Exception as e:
                raise RuntimeError(f"Failed to create REPL session: {e}")
        return self._session

    def _find_project_path(self) -> str:
        """Find the user's project path from frame or environment."""
        if self.project_path:
            return self.project_path

        # Try to get from frame's compile unit
        if self.frame:
            compile_unit = self.frame.GetCompileUnit()
            if compile_unit:
                file_spec = compile_unit.GetFileSpec()
                if file_spec:
                    source_dir = os.path.dirname(file_spec.GetDirectory())
                    # Walk up to find Cargo.toml
                    current = source_dir
                    for _ in range(10):
                        cargo_toml = os.path.join(current, "Cargo.toml")
                        if os.path.exists(cargo_toml):
                            return current
                        parent = os.path.dirname(current)
                        if parent == current:
                            break
                        current = parent

        # Fallback: try current directory
        if os.path.exists("Cargo.toml"):
            return os.getcwd()

        return None

    def _generate_companion_lib(self) -> tuple:
        """
        Generate companion lib from user project.

        Returns:
            Tuple of (lib_path, crate_name) or (None, None) if fails
        """
        project_path = self._find_project_path()
        if not project_path:
            return None, None

        try:
            from .ferrumpy_core import generate_lib
            lib_path, crate_name = generate_lib(project_path, None)
            return lib_path, crate_name
        except Exception as e:
            print(f"Warning: Failed to generate companion lib: {e}")
            return None, None

    def initialize(self) -> str:
        """
        Initialize the REPL with captured frame variables.

        This method:
        1. Generates a companion lib from the user's project
        2. Loads it as a dependency so user types are available
        3. Loads variable snapshot from the frame

        Returns:
            Status message
        """
        session = self._get_rust_session()

        # Step 1: Generate companion lib (for user types)
        self._lib_path, self._lib_name = self._generate_companion_lib()

        # Step 2: Register lib dep silently (no compilation yet)
        lib_use_stmt = ""
        if self._lib_path and self._lib_name:
            try:
                # Use silent mode - no compilation until load_snapshot
                session.add_path_dep_silent(self._lib_name, self._lib_path)
                lib_use_stmt = f"use {self._lib_name}::*;"
            except Exception as e:
                print(f"Warning: Failed to register companion lib: {e}")

        # Step 3: Load variable snapshot (single compilation with all deps)
        if self.frame:
            data = serialize_frame(self.frame)
            # Add lib metadata to snapshot for potential restoration after interrupt
            if lib_use_stmt:
                data['lib_use_stmt'] = lib_use_stmt
            if self._lib_path:
                data['lib_path'] = str(self._lib_path)
            if self._lib_name:
                data['lib_name'] = self._lib_name

            json_data = json.dumps(data)
            type_hints = ",".join(
                f"{k}:{v}" for k, v in data.get('types', {}).items()
            )
            result = session.load_snapshot(json_data, type_hints)
            self._initialized = True

            # Note: OutputDrainer is disabled - evcxr handles draining internally

            return result
        else:
            # If no frame, still need to compile lib dep
            if lib_use_stmt:
                session.eval(lib_use_stmt)
            self._initialized = True

            # Note: OutputDrainer is disabled - evcxr handles draining internally

            return "Initialized (no frame data)"

    def eval(self, code: str) -> str:
        """
        Evaluate Rust code in the REPL.

        Args:
            code: Rust code to evaluate

        Returns:
            Evaluation result
        """
        session = self._get_rust_session()

        # Note: Output draining happens automatically in evcxr's background threads.
        # We don't call _drain_pending_output() here because print() conflicts
        # with prompt_toolkit in enhanced mode, causing deadlock.

        try:
            result = session.eval(code)
            return result
        except Exception as e:
            error_msg = str(e)
            # Check if the subprocess was terminated (e.g., by SIGINT)
            if "Subprocess terminated" in error_msg or "SIGINT" in error_msg:
                # Subprocess was killed, restore the snapshot
                try:
                    session.interrupt()  # This will restore the snapshot
                    # Note: OutputDrainer disabled - no need to restart
                except Exception as restore_error:
                    # If restoration fails, re-raise the original error
                    raise Exception(f"{error_msg}\n(Failed to restore snapshot: {restore_error})")
            # Re-raise the original error
            raise

    def _drain_pending_output(self):
        """
        Drain and display any pending output from the subprocess.

        This handles output from background threads or async operations that may
        arrive after eval() returns. Output is displayed immediately to stdout/stderr.
        """
        # Skip draining if session not initialized yet
        if not self._initialized or self._session is None:
            return

        session = self._get_rust_session()

        # Drain stdout
        stdout_lines = session.drain_stdout()
        for line in stdout_lines:
            print(line, flush=True)

        # Drain stderr
        stderr_lines = session.drain_stderr()
        for line in stderr_lines:
            print(line, file=sys.stderr, flush=True)

    def add_dep(self, name: str, spec: str) -> str:
        """
        Add a crate dependency.

        Args:
            name: Crate name
            spec: Version spec (e.g., '"1.0"' or '{ version = "1", features = ["derive"] }')

        Returns:
            Result message
        """
        session = self._get_rust_session()
        return session.add_dep(name, spec)

    def is_initialized(self) -> bool:
        """Check if the session has been initialized with frame data."""
        if self._session:
            return self._session.is_initialized()
        return False

    def get_stderr(self) -> list:
        """Get any stderr output from the REPL."""
        if self._session:
            return self._session.get_stderr()
        return []

    def run_interactive(self):
        """
        Run an interactive REPL loop.

        This provides a simple command-line interface to the embedded REPL.
        """
        if not self._initialized:
            self.initialize()

        print("FerrumPy Embedded REPL")
        print("Type Rust expressions. Use :q or :exit to quit.")
        print("-" * 40)

        while True:
            try:
                code = input(">> ")
            except (EOFError, KeyboardInterrupt):
                print("\nExiting...")
                break

            code = code.strip()
            if not code:
                continue

            if code in (':q', ':quit', ':exit'):
                break

            try:
                result = self.eval(code)
                if result:
                    print(result)
            except Exception as e:
                print(f"Error: {e}")

        print("REPL session ended.")


def start_embedded_repl(frame=None) -> EmbeddedReplSession:
    """
    Create and initialize an embedded REPL session.

    Args:
        frame: Optional LLDB SBFrame to capture variables from

    Returns:
        Initialized EmbeddedReplSession
    """
    session = EmbeddedReplSession(frame)
    session.initialize()
    return session

