#!/usr/bin/env python3
"""Antigravity Token Extractor

Extracts OAuth tokens from Gemini CLI or other Google tools.

Strategy:
1. Check known token storage locations
2. If not found, run Gemini CLI to trigger auth
3. Watch for token file creation
4. Extract and normalize token for op-dbus

Usage:
    ./antigravity-token-extractor.py
    ./antigravity-token-extractor.py --watch
    ./antigravity-token-extractor.py --run-gemini
"""

import argparse
import asyncio
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import Optional, Dict, Any, List
import shutil

# =============================================================================
# CONFIGURATION
# =============================================================================

# Output location for op-dbus
OPDBUS_TOKEN_FILE = Path.home() / ".config" / "antigravity" / "token.json"

# Known token storage locations for various Google tools
TOKEN_PATHS = [
    # Gemini CLI locations
    Path.home() / ".gemini" / "oauth_token.json",
    Path.home() / ".gemini" / "credentials.json",
    Path.home() / ".config" / "gemini" / "credentials.json",
    Path.home() / ".config" / "gemini-cli" / "credentials.json",
    Path.home() / ".config" / "google-gemini" / "oauth.json",
    Path.home() / ".cache" / "gemini" / "auth.json",
    Path.home() / ".local" / "share" / "gemini" / "token.json",
    
    # Google AI SDK
    Path.home() / ".config" / "google-generativeai" / "credentials.json",
    
    # gcloud ADC
    Path.home() / ".config" / "gcloud" / "application_default_credentials.json",
    
    # Firebase
    Path.home() / ".config" / "firebase" / "tokens.json",
]

# Commands that might contain Gemini CLI
GEMINI_COMMANDS = [
    "gemini",
    "gemini-cli",
    "google-gemini",
    "genai",
]

# =============================================================================
# TOKEN EXTRACTION
# =============================================================================

def find_gemini_command() -> Optional[str]:
    """Find the Gemini CLI command if installed."""
    for cmd in GEMINI_COMMANDS:
        if shutil.which(cmd):
            return cmd
    return None


def find_existing_token() -> Optional[Path]:
    """Check known paths for existing tokens."""
    for path in TOKEN_PATHS:
        if path.exists():
            try:
                with open(path) as f:
                    data = json.load(f)
                    # Check if it has useful token data
                    if 'access_token' in data or 'refresh_token' in data:
                        print(f"✅ Found token at: {path}")
                        return path
            except (json.JSONDecodeError, IOError):
                continue
    return None


def normalize_token(data: Dict[str, Any], source: str) -> Dict[str, Any]:
    """Normalize token to a consistent format."""
    token = {
        'access_token': data.get('access_token', ''),
        'refresh_token': data.get('refresh_token', ''),
        'token_type': data.get('token_type', 'Bearer'),
        'scope': data.get('scope', ''),
        'saved_at': time.time(),
        'source': source,
    }
    
    # Copy optional fields
    for key in ['client_id', 'client_secret', 'quota_project_id', 'expires_in']:
        if key in data:
            token[key] = data[key]
    
    # Calculate expiry
    if 'expires_at' in data:
        token['expires_at'] = data['expires_at']
    elif 'expires_in' in data:
        token['expires_at'] = time.time() + int(data['expires_in'])
    elif 'expiry' in data:
        # Handle ISO format expiry
        try:
            from datetime import datetime
            exp = datetime.fromisoformat(data['expiry'].replace('Z', '+00:00'))
            token['expires_at'] = exp.timestamp()
        except:
            pass
    
    return token


def save_token(token: Dict[str, Any]) -> None:
    """Save token to op-dbus location."""
    OPDBUS_TOKEN_FILE.parent.mkdir(parents=True, exist_ok=True)
    
    with open(OPDBUS_TOKEN_FILE, 'w') as f:
        json.dump(token, f, indent=2)
    
    os.chmod(OPDBUS_TOKEN_FILE, 0o600)
    print(f"✅ Token saved to: {OPDBUS_TOKEN_FILE}")


def extract_token_from_path(path: Path) -> bool:
    """Extract and save token from a file path."""
    try:
        with open(path) as f:
            data = json.load(f)
        
        token = normalize_token(data, str(path))
        save_token(token)
        return True
    except Exception as e:
        print(f"❌ Failed to extract token from {path}: {e}")
        return False


# =============================================================================
# WATCH MODE
# =============================================================================

def watch_for_tokens(timeout_secs: int = 300) -> bool:
    """Watch for token files to appear."""
    print(f"👀 Watching for token files (timeout: {timeout_secs}s)...")
    
    # Get initial mtimes
    initial_mtimes = {}
    for path in TOKEN_PATHS:
        if path.exists():
            initial_mtimes[path] = path.stat().st_mtime
    
    start_time = time.time()
    while time.time() - start_time < timeout_secs:
        for path in TOKEN_PATHS:
            if path.exists():
                current_mtime = path.stat().st_mtime
                # Check if file is new or modified
                if path not in initial_mtimes or current_mtime > initial_mtimes.get(path, 0):
                    # Verify it has valid content
                    try:
                        with open(path) as f:
                            data = json.load(f)
                        if 'access_token' in data or 'refresh_token' in data:
                            print(f"\n✅ Token file detected: {path}")
                            return extract_token_from_path(path)
                    except:
                        pass
        
        print(".", end="", flush=True)
        time.sleep(1)
    
    print("\n❌ Timeout waiting for token")
    return False


# =============================================================================
# RUN GEMINI CLI
# =============================================================================

async def run_gemini_and_capture():
    """Run Gemini CLI and capture the OAuth token it creates."""
    gemini_cmd = find_gemini_command()
    if not gemini_cmd:
        print("❌ Gemini CLI not found")
        return False
    
    print(f"🚀 Found Gemini CLI: {gemini_cmd}")
    print("   This will launch Gemini CLI which may trigger OAuth...")
    
    # Start watching for tokens in background
    watch_task = asyncio.create_task(asyncio.to_thread(watch_for_tokens, 120))
    
    # Run Gemini CLI
    try:
        # Try running a simple command to trigger auth
        proc = await asyncio.create_subprocess_exec(
            gemini_cmd, "--help",
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        await asyncio.wait_for(proc.wait(), timeout=30)
    except asyncio.TimeoutError:
        print("   --help timed out, trying interactive...")
    except Exception as e:
        print(f"   Error running --help: {e}")
    
    # If no token yet, try interactive
    if not watch_task.done():
        try:
            print("   Trying interactive query...")
            proc = await asyncio.create_subprocess_exec(
                gemini_cmd,
                stdin=asyncio.subprocess.PIPE,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
            stdout, stderr = await asyncio.wait_for(
                proc.communicate(input=b"Hello\n"),
                timeout=60
            )
        except asyncio.TimeoutError:
            print("   Interactive query timed out")
        except Exception as e:
            print(f"   Error: {e}")
    
    # Wait for watch task
    try:
        result = await asyncio.wait_for(watch_task, timeout=10)
        return result
    except asyncio.TimeoutError:
        watch_task.cancel()
        return False


# =============================================================================
# MAIN
# =============================================================================

def main():
    parser = argparse.ArgumentParser(
        description="Extract OAuth token from Gemini CLI for op-dbus"
    )
    parser.add_argument("--watch", action="store_true",
                       help="Watch for token files to appear")
    parser.add_argument("--run-gemini", action="store_true",
                       help="Run Gemini CLI to trigger OAuth")
    parser.add_argument("--timeout", type=int, default=300,
                       help="Timeout in seconds for watch mode")
    parser.add_argument("--output", "-o", type=Path, default=OPDBUS_TOKEN_FILE,
                       help="Output file path")
    
    args = parser.parse_args()
    
    global OPDBUS_TOKEN_FILE
    OPDBUS_TOKEN_FILE = args.output
    
    print("\n╔════════════════════════════════════════════════════════════════╗")
    print("║     ANTIGRAVITY TOKEN EXTRACTOR                                ║")
    print("╚════════════════════════════════════════════════════════════════╝\n")
    
    # Check for existing token first
    existing = find_existing_token()
    if existing:
        if extract_token_from_path(existing):
            print_success()
            return 0
    
    # Watch mode
    if args.watch:
        if watch_for_tokens(args.timeout):
            print_success()
            return 0
        return 1
    
    # Run Gemini mode
    if args.run_gemini:
        if asyncio.run(run_gemini_and_capture()):
            print_success()
            return 0
        return 1
    
    # Default: try to run Gemini CLI
    gemini_cmd = find_gemini_command()
    if gemini_cmd:
        print(f"Found Gemini CLI: {gemini_cmd}")
        print("Attempting to trigger OAuth...\n")
        if asyncio.run(run_gemini_and_capture()):
            print_success()
            return 0
    
    # If no Gemini CLI, just watch
    print("\nGemini CLI not found. Watching for token files...")
    print("(Run the Gemini CLI manually in another terminal)\n")
    if watch_for_tokens(args.timeout):
        print_success()
        return 0
    
    print("\n❌ Could not capture OAuth token")
    return 1


def print_success():
    print("\n" + "═" * 64)
    print("✅ Token captured successfully!")
    print("")
    print(f"   Token file: {OPDBUS_TOKEN_FILE}")
    print("")
    print("   To use with op-dbus:")
    print(f"     export GOOGLE_AUTH_TOKEN_FILE={OPDBUS_TOKEN_FILE}")
    print("     export LLM_PROVIDER=antigravity")
    print("═" * 64)


if __name__ == "__main__":
    sys.exit(main())
