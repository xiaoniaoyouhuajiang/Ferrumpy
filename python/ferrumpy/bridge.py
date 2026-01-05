"""
Bridge to ferrumpy-server

Manages the ferrumpy-server subprocess and provides JSON-RPC communication.
"""
import json
import os
import subprocess
from typing import Any, Dict, List, Optional

# Path to ferrumpy-server binary
_SERVER_BINARY = None


def _find_server_binary() -> str:
    """Find the ferrumpy-server binary."""
    global _SERVER_BINARY
    if _SERVER_BINARY:
        return _SERVER_BINARY

    # Look in common locations
    candidates = [
        # Development build
        os.path.join(os.path.dirname(__file__), "..", "..", "target", "debug", "ferrumpy-server"),
        os.path.join(os.path.dirname(__file__), "..", "..", "target", "release", "ferrumpy-server"),
        # Installed in PATH
        "ferrumpy-server",
    ]

    for candidate in candidates:
        if os.path.isfile(candidate) and os.access(candidate, os.X_OK):
            _SERVER_BINARY = candidate
            return candidate

    raise FileNotFoundError("ferrumpy-server binary not found. Run 'cargo build' first.")


class ServerConnection:
    """
    Connection to ferrumpy-server subprocess.

    Uses stdin/stdout for JSON-RPC communication.
    """

    def __init__(self):
        self._process: Optional[subprocess.Popen] = None
        self._request_id = 0
        self._initialized = False

    def start(self) -> None:
        """Start the server subprocess."""
        if self._process is not None:
            return

        binary = _find_server_binary()
        self._process = subprocess.Popen(
            [binary],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,  # Line buffered
        )

    def stop(self) -> None:
        """Stop the server subprocess."""
        if self._process is None:
            return

        try:
            self._send_request("shutdown", {})
        except Exception:
            pass

        self._process.terminate()
        self._process.wait(timeout=5)
        self._process = None
        self._initialized = False

    def _send_request(self, method: str, params: Dict[str, Any]) -> Dict[str, Any]:
        """Send a JSON-RPC request and get response."""
        if self._process is None:
            raise RuntimeError("Server not started")

        self._request_id += 1
        request = {
            "jsonrpc": "2.0",
            "id": self._request_id,
            "method": method,
            "params": params,
        }

        request_json = json.dumps(request)
        self._process.stdin.write(request_json + "\n")
        self._process.stdin.flush()

        response_line = self._process.stdout.readline()
        if not response_line:
            raise RuntimeError("Server closed connection")

        response = json.loads(response_line)
        return response

    def initialize(self, project_root: str) -> bool:
        """Initialize the server for a project."""
        self.start()

        response = self._send_request("initialize", {"project_root": project_root})

        # Handle both response formats: {ok: true} and {result: {ok: true}}
        if response.get("ok"):
            self._initialized = True
            return True
        if "result" in response and response["result"].get("ok"):
            self._initialized = True
            return True

        return False

    def complete(self, frame_info: Dict, input_text: str, cursor: int) -> List[Dict]:
        """Request completions."""
        if not self._initialized:
            return []

        response = self._send_request("complete", {
            "frame": frame_info,
            "input": input_text,
            "cursor": cursor,
        })

        if "result" in response and "completions" in response["result"]:
            return response["result"]["completions"]

        return []

    def type_info(self, frame_info: Dict, expr: str) -> Optional[str]:
        """Get type information for an expression."""
        if not self._initialized:
            return None

        response = self._send_request("type", {
            "frame": frame_info,
            "expr": expr,
        })

        if "result" in response and "type_name" in response["result"]:
            return response["result"]["type_name"]

        return None

    def eval(self, frame_info: Dict, expr: str) -> Optional[Dict]:
        """Evaluate an expression."""
        if not self._initialized:
            return None

        response = self._send_request("eval", {
            "frame": frame_info,
            "expr": expr,
        })

        # Handle flat response format: {value: ..., value_type: ...}
        if "value" in response:
            return {"value": response["value"], "value_type": response.get("value_type", "")}
        if "error" in response:
            return {"error": response["error"]}

        # Handle nested format: {result: {value: ...}}
        if "result" in response:
            result = response["result"]
            if "value" in result:
                return result
            if "error" in result:
                return {"error": result["error"]}

        return None


# Global connection instance
_connection: Optional[ServerConnection] = None


def get_connection() -> ServerConnection:
    """Get the global server connection."""
    global _connection
    if _connection is None:
        _connection = ServerConnection()
    return _connection


def frame_to_info(frame) -> Dict[str, Any]:
    """
    Convert LLDB SBFrame to FrameInfo dict.

    Args:
        frame: lldb.SBFrame object

    Returns:
        Dict with function, file, line, and locals
    """

    info = {
        "function": frame.GetFunctionName() or "unknown",
        "file": None,
        "line": None,
        "locals": [],
    }

    # Get source location
    line_entry = frame.GetLineEntry()
    if line_entry.IsValid():
        file_spec = line_entry.GetFileSpec()
        if file_spec.IsValid():
            info["file"] = str(file_spec)
        info["line"] = line_entry.GetLine()

    # Get local variables
    variables = frame.GetVariables(True, True, False, True)  # locals, args, statics, scope
    for i in range(variables.GetSize()):
        var = variables.GetValueAtIndex(i)
        if var.IsValid():
            type_name = var.GetType().GetName()
            # Convert DWARF type name to Rust syntax (simplified)
            rust_type = _simplify_type_name(type_name)

            # Get value for primitive types
            value_str = ""
            if rust_type in ("i8", "i16", "i32", "i64", "i128", "isize",
                           "u8", "u16", "u32", "u64", "u128", "usize",
                           "f32", "f64", "bool"):
                value_str = var.GetValue() or ""

            info["locals"].append({
                "name": var.GetName(),
                "type_name": type_name,
                "rust_type": rust_type,
                "value": value_str,
            })

    return info


def _simplify_type_name(type_name: str) -> str:
    """Simplify DWARF type name to Rust syntax."""
    replacements = [
        ("alloc::string::", ""),
        ("alloc::vec::", ""),
        ("alloc::boxed::", ""),
        ("alloc::sync::", ""),
        ("alloc::rc::", ""),
        ("core::option::", ""),
        ("core::result::", ""),
    ]

    result = type_name
    for old, new in replacements:
        result = result.replace(old, new)

    return result

