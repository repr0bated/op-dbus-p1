#!/usr/bin/env python3
"""Antigravity Replay Client

Replays captured Antigravity IDE requests with the same headers.
This allows op-dbus to use the IDE's Code Assist subscription.

Usage:
    # After capturing with antigravity-proxy-capture.sh:
    export ANTIGRAVITY_SESSION_FILE=~/.config/antigravity/captured/session.json
    
    # Use as LLM provider:
    from antigravity_replay_client import AntigravityReplayClient
    client = AntigravityReplayClient.from_captured_session()
    response = await client.chat("Hello!")
"""

import asyncio
import json
import os
import sys
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Any, Dict, List, Optional

import aiohttp

# =============================================================================
# CONFIGURATION
# =============================================================================

DEFAULT_SESSION_FILE = Path.home() / ".config" / "antigravity" / "captured" / "session.json"
DEFAULT_HEADERS_FILE = Path.home() / ".config" / "antigravity" / "captured" / "headers.json"
DEFAULT_TOKEN_FILE = Path.home() / ".config" / "antigravity" / "captured" / "token.json"

# Gemini API endpoint (what Antigravity actually uses)
GEMINI_API_BASE = "https://generativelanguage.googleapis.com/v1beta"

# Code Assist endpoint (enterprise feature)
CODE_ASSIST_BASE = "https://cloudaicompanion.googleapis.com/v1"


@dataclass
class CapturedSession:
    """Captured Antigravity session with auth and headers"""
    access_token: str
    headers: Dict[str, str]
    endpoints: List[Dict[str, Any]] = field(default_factory=list)
    captured_at: Optional[str] = None
    
    @classmethod
    def from_files(cls, session_file: Optional[Path] = None) -> "CapturedSession":
        """Load from captured session files"""
        session_file = session_file or DEFAULT_SESSION_FILE
        
        if not session_file.exists():
            raise FileNotFoundError(
                f"Session file not found: {session_file}\n"
                f"Run antigravity-proxy-capture.sh first to capture credentials."
            )
        
        with open(session_file) as f:
            data = json.load(f)
        
        # Extract latest token
        tokens = data.get("tokens", [])
        if not tokens:
            raise ValueError("No tokens found in session file")
        
        latest_token = tokens[-1]
        access_token = latest_token.get("access_token", "")
        
        # Get captured headers
        headers = data.get("headers", {})
        
        # Also try dedicated headers file
        if DEFAULT_HEADERS_FILE.exists():
            with open(DEFAULT_HEADERS_FILE) as f:
                file_headers = json.load(f)
                headers.update(file_headers)
        
        return cls(
            access_token=access_token,
            headers=headers,
            endpoints=data.get("endpoints", []),
            captured_at=latest_token.get("captured_at"),
        )
    
    def get_request_headers(self) -> Dict[str, str]:
        """Get headers for making requests"""
        headers = {
            "Authorization": f"Bearer {self.access_token}",
            "Content-Type": "application/json",
        }
        
        # Add captured headers (these are the IDE-identifying headers)
        for key, value in self.headers.items():
            # Skip authorization (we set it above)
            if key.lower() == "authorization":
                continue
            headers[key] = value
        
        return headers


class AntigravityReplayClient:
    """Client that replays Antigravity IDE requests
    
    This client uses captured OAuth token and headers from Antigravity IDE
    to make requests that appear to come from the IDE, thus getting
    Code Assist subscription benefits.
    """
    
    def __init__(self, session: CapturedSession):
        self.session = session
        self._http_session: Optional[aiohttp.ClientSession] = None
    
    @classmethod
    def from_captured_session(
        cls, 
        session_file: Optional[Path] = None
    ) -> "AntigravityReplayClient":
        """Create client from captured session files"""
        session = CapturedSession.from_files(session_file)
        return cls(session)
    
    @classmethod
    def from_env(cls) -> "AntigravityReplayClient":
        """Create client from environment variable"""
        session_file = os.environ.get("ANTIGRAVITY_SESSION_FILE")
        if session_file:
            return cls.from_captured_session(Path(session_file))
        return cls.from_captured_session()
    
    async def _get_http_session(self) -> aiohttp.ClientSession:
        """Get or create HTTP session"""
        if self._http_session is None or self._http_session.closed:
            self._http_session = aiohttp.ClientSession(
                headers=self.session.get_request_headers(),
                timeout=aiohttp.ClientTimeout(total=120),
            )
        return self._http_session
    
    async def close(self):
        """Close HTTP session"""
        if self._http_session and not self._http_session.closed:
            await self._http_session.close()
    
    async def chat(
        self,
        message: str,
        model: str = "gemini-2.0-flash",
        system_prompt: Optional[str] = None,
        history: Optional[List[Dict[str, str]]] = None,
    ) -> Dict[str, Any]:
        """Send chat message using captured Antigravity credentials
        
        Args:
            message: User message
            model: Model to use (default: gemini-2.0-flash)
            system_prompt: Optional system prompt
            history: Optional conversation history
            
        Returns:
            Response dict with 'text' and 'model' keys
        """
        session = await self._get_http_session()
        
        # Build Gemini API request
        contents = []
        
        # Add history
        if history:
            for msg in history:
                role = "model" if msg.get("role") == "assistant" else "user"
                contents.append({
                    "role": role,
                    "parts": [{"text": msg.get("content", "")}]
                })
        
        # Add current message
        contents.append({
            "role": "user",
            "parts": [{"text": message}]
        })
        
        request_body = {
            "contents": contents,
        }
        
        # Add system instruction if provided
        if system_prompt:
            request_body["systemInstruction"] = {
                "parts": [{"text": system_prompt}]
            }
        
        # Use the endpoint format we captured from Antigravity
        # This might be different from the standard Gemini API
        url = f"{GEMINI_API_BASE}/models/{model}:generateContent"
        
        print(f"[REPLAY] POST {url}")
        print(f"[REPLAY] Headers: {list(session.headers.keys())}")
        
        async with session.post(url, json=request_body) as response:
            if response.status != 200:
                error_text = await response.text()
                raise RuntimeError(f"API error {response.status}: {error_text}")
            
            result = await response.json()
        
        # Parse response
        candidates = result.get("candidates", [])
        if not candidates:
            raise RuntimeError("No response candidates")
        
        text_parts = []
        for part in candidates[0].get("content", {}).get("parts", []):
            if "text" in part:
                text_parts.append(part["text"])
        
        return {
            "text": "".join(text_parts),
            "model": model,
            "usage": result.get("usageMetadata", {}),
        }
    
    async def list_models(self) -> List[Dict[str, Any]]:
        """List available models"""
        session = await self._get_http_session()
        
        url = f"{GEMINI_API_BASE}/models"
        
        async with session.get(url) as response:
            if response.status != 200:
                error_text = await response.text()
                raise RuntimeError(f"API error {response.status}: {error_text}")
            
            result = await response.json()
        
        return result.get("models", [])
    
    def get_captured_headers(self) -> Dict[str, str]:
        """Get the captured IDE headers (for debugging)"""
        return self.session.headers.copy()


# =============================================================================
# CLI
# =============================================================================

async def main():
    import argparse
    
    parser = argparse.ArgumentParser(
        description="Replay Antigravity IDE requests"
    )
    parser.add_argument(
        "--session", "-s",
        type=Path,
        default=DEFAULT_SESSION_FILE,
        help="Path to captured session file"
    )
    parser.add_argument(
        "--model", "-m",
        default="gemini-2.0-flash",
        help="Model to use"
    )
    parser.add_argument(
        "--list-models",
        action="store_true",
        help="List available models"
    )
    parser.add_argument(
        "--show-headers",
        action="store_true",
        help="Show captured headers"
    )
    parser.add_argument(
        "message",
        nargs="?",
        help="Message to send"
    )
    
    args = parser.parse_args()
    
    try:
        client = AntigravityReplayClient.from_captured_session(args.session)
    except FileNotFoundError as e:
        print(f"❌ {e}")
        sys.exit(1)
    
    try:
        if args.show_headers:
            print("\nCaptured IDE Headers:")
            print("=" * 60)
            for key, value in client.get_captured_headers().items():
                # Truncate long values
                display_value = value[:60] + "..." if len(value) > 60 else value
                print(f"  {key}: {display_value}")
            return
        
        if args.list_models:
            print("\nAvailable Models:")
            print("=" * 60)
            models = await client.list_models()
            for model in models:
                print(f"  - {model.get('name', 'unknown')}")
                if desc := model.get('description'):
                    print(f"    {desc[:60]}..." if len(desc) > 60 else f"    {desc}")
            return
        
        if not args.message:
            # Interactive mode
            print("\n🤖 Antigravity Replay Client (Interactive Mode)")
            print("   Using captured IDE credentials")
            print("   Type 'quit' to exit\n")
            
            while True:
                try:
                    message = input("You: ").strip()
                    if message.lower() in ("quit", "exit", "q"):
                        break
                    if not message:
                        continue
                    
                    response = await client.chat(message, model=args.model)
                    print(f"\nAssistant: {response['text']}\n")
                    
                except KeyboardInterrupt:
                    print("\n")
                    break
        else:
            # Single message mode
            response = await client.chat(args.message, model=args.model)
            print(response["text"])
    
    finally:
        await client.close()


if __name__ == "__main__":
    asyncio.run(main())
